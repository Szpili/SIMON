//! F3 tests in Rust - verifying signed receipt integrity.
//!
//! F3 attack vector: "complete() does not validate signature" -> forgery vulnerability.
//! Each test corresponds to a specific attack scenario from red-team evaluation.

use simon_core::crypto::Keypair;
use simon_core::receipt::{Precision, Receipt};
use simon_core::{SimonError, PROTOCOL_VERSION};
use simon_node::{Capability, JobResult, NodeState, RegisterMsg, Registry};

fn receipt_for(job_id: &str, node_id: &str) -> Receipt {
    Receipt {
        job_id: job_id.to_string(),
        node_id: node_id.to_string(),
        model_hash: "sha256:model".to_string(),
        runtime: "vllm-0.6.3".to_string(),
        precision: Precision::Bf16,
        activation_hash: "toploc:258B".to_string(),
        output_digest: "sha256:out".to_string(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 100_000_000,
        finished_at_us: 100_500_000,
        signer: Keypair::generate().public(),
        signature: None,
        bindings: None,
    }
}

fn registry_with_node(attested: bool) -> (Registry, Keypair, String) {
    let mut reg = Registry::new();
    let key = Keypair::generate();
    let node_id = "node-1".to_string();
    let cap = Capability {
        declared: serde_json::json!({"vram_gb": 24}),
        attested: if attested {
            Some(serde_json::json!({"tok_s": 48.2}))
        } else {
            None
        },
    };
    let msg = RegisterMsg::new(node_id.clone(), key.public(), cap);
    assert!(reg.register(msg), "registration should succeed");
    (reg, key, node_id)
}

// --- Capability gate: assignment only with attested capability ---

#[test]
fn assign_requires_attested_capability() {
    let (mut reg, _, node_id) = registry_with_node(false);
    assert!(!reg.assign("job-1", &node_id), "unattested node cannot receive jobs");
}

#[test]
fn assign_after_attestation_ok() {
    let (mut reg, _, node_id) = registry_with_node(true);
    assert!(reg.assign("job-1", &node_id));
}

// --- F3: receipts ---

#[test]
fn unsigned_receipt_does_not_verify() {
    let r = receipt_for("job-1", "node-1");
    assert!(matches!(r.verify_self(), Err(SimonError::BadSignature)));
}

#[test]
fn signed_receipt_verifies_self() {
    let key = Keypair::generate();
    let r = receipt_for("job-1", "node-1").sign(&key).unwrap();
    assert!(r.verify_self().is_ok());
    assert_eq!(r.signer, key.public());
}

#[test]
fn tampered_content_rejected() {
    let key = Keypair::generate();
    let mut r = receipt_for("job-1", "node-1").sign(&key).unwrap();
    r.output_digest = "sha256:TAMPERED".to_string();
    assert!(
        matches!(r.verify_self(), Err(SimonError::BadSignature)),
        "tampering content after signing must be detected"
    );
}

#[test]
fn signature_not_transferable_to_other_receipt() {
    let key = Keypair::generate();
    let signed = receipt_for("job-1", "node-1").sign(&key).unwrap();
    // Same signature, different job_id.
    let mut other = signed.clone();
    other.job_id = "job-2".to_string();
    assert!(other.verify_self().is_err());
}

#[test]
fn digest_is_deterministic() {
    let mut a = receipt_for("job-1", "node-1");
    let mut b = receipt_for("job-1", "node-1");
    a.signer = b.signer.clone();
    let _ = (&mut a, &mut b);
    assert_eq!(a.digest().unwrap(), b.digest().unwrap());
}

#[test]
fn signature_excluded_from_digest() {
    let key = Keypair::generate();
    let mut signed = receipt_for("job-1", "node-1").sign(&key).unwrap();
    let with_sig = signed.digest().unwrap();
    signed.signature = None;
    let without_sig = signed.digest().unwrap();
    assert_eq!(with_sig, without_sig, "signature must not be included in digest");
}

// --- F3 integrated in complete() ---

#[test]
fn complete_rejects_ok_without_receipt() {
    let (mut reg, _, node_id) = registry_with_node(true);
    assert!(reg.assign("job-1", &node_id));
    let res = JobResult {
        job_id: "job-1".into(),
        node_id: node_id.clone(),
        ok: true,
        output: Some(serde_json::json!({})),
        receipt: None,
        error: None,
        signature: None,
    };
    assert!(
        reg.complete(&res).is_err(),
        "successful result without receipt must be rejected"
    );
}

#[test]
fn complete_accepts_valid_signed_receipt() {
    let (mut reg, key, node_id) = registry_with_node(true);
    assert!(reg.assign("job-1", &node_id));
    let receipt = receipt_for("job-1", &node_id).sign(&key).unwrap();
    let res = JobResult {
        job_id: "job-1".into(),
        node_id: node_id.clone(),
        ok: true,
        output: Some(serde_json::json!({"logits": [0.1]})),
        receipt: Some(receipt),
        error: None,
        signature: None,
    };
    assert!(reg.complete(&res).is_ok());
}

#[test]
fn complete_rejects_receipt_signed_by_foreign_key() {
    // Attack: adversary signs with foreign key and embeds foreign pubkey.
    let (mut reg, _registered_key, node_id) = registry_with_node(true);
    assert!(reg.assign("job-1", &node_id));

    let foreign_key = Keypair::generate();
    let forged = receipt_for("job-1", &node_id).sign(&foreign_key).unwrap();
    assert!(forged.verify_self().is_ok(), "signature is internally valid");

    let res = JobResult {
        job_id: "job-1".into(),
        node_id: node_id.clone(),
        ok: true,
        output: Some(serde_json::json!({})),
        receipt: Some(forged),
        error: None,
        signature: None,
    };
    assert!(
        matches!(reg.complete(&res), Err(SimonError::UnregisteredKey)),
        "signer key must be registered"
    );
}

#[test]
fn complete_rejects_forged_receipt_content() {
    let (mut reg, key, node_id) = registry_with_node(true);
    assert!(reg.assign("job-1", &node_id));
    let mut receipt = receipt_for("job-1", &node_id).sign(&key).unwrap();
    receipt.activation_hash = "toploc:FAKE".into();
    let res = JobResult {
        job_id: "job-1".into(),
        node_id: node_id.clone(),
        ok: true,
        output: Some(serde_json::json!({})),
        receipt: Some(receipt),
        error: None,
        signature: None,
    };
    assert!(reg.complete(&res).is_err());
}

#[test]
fn complete_rejects_receipt_for_other_job() {
    let (mut reg, key, node_id) = registry_with_node(true);
    assert!(reg.assign("job-1", &node_id));
    let receipt = receipt_for("job-OTHER", &node_id).sign(&key).unwrap();
    let res = JobResult {
        job_id: "job-1".into(),
        node_id: node_id.clone(),
        ok: true,
        output: Some(serde_json::json!({})),
        receipt: Some(receipt),
        error: None,
        signature: None,
    };
    assert!(reg.complete(&res).is_err(), "job_id mismatch must be rejected");
}

#[test]
fn complete_allows_failure_without_receipt() {
    let (mut reg, _, node_id) = registry_with_node(true);
    assert!(reg.assign("job-1", &node_id));
    let res = JobResult {
        job_id: "job-1".into(),
        node_id: node_id.clone(),
        ok: false,
        output: None,
        receipt: None,
        error: Some("OOM".into()),
        signature: None,
    };
    assert!(reg.complete(&res).is_ok(), "failure result does not require receipt");
}

#[test]
fn complete_rejects_result_from_unassigned_node() {
    let (mut reg, key, _) = registry_with_node(true);
    let receipt = receipt_for("job-1", "node-1").sign(&key).unwrap();
    let res = JobResult {
        job_id: "job-1".into(),
        node_id: "node-1".into(),
        ok: true,
        output: Some(serde_json::json!({})),
        receipt: Some(receipt),
        error: None,
        signature: None,
    };
    assert!(reg.complete(&res).is_err());
}

// --- degradation ---

#[test]
fn evict_orphans_jobs() {
    let (mut reg, _, node_id) = registry_with_node(true);
    assert!(reg.assign("job-1", &node_id));
    let orphaned = reg.evict(&node_id);
    assert_eq!(orphaned, vec!["job-1".to_string()]);
    assert_eq!(reg.states.get(&node_id), Some(&NodeState::Evicted));
}

#[test]
fn evicted_node_cannot_get_job() {
    let (mut reg, _, node_id) = registry_with_node(true);
    reg.evict(&node_id);
    assert!(!reg.assign("job-2", &node_id));
}

#[test]
fn protocol_version_is_frozen() {
    assert_eq!(PROTOCOL_VERSION, "simon/v1");
}
