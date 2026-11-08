//! Integration tests: full cycle order -> execution -> client-side verification.
//!
//! Verifies the contract between three layers:
//!   harness (client) -> node (coordinator + worker) -> back to client
//!
//! Confirms the client verifies the outcome autonomously without trusting the coordinator.

use simon_core::crypto::Keypair;
use simon_core::receipt::{Precision, Receipt};
use simon_core::SimonError;
use simon_harness::client_protocol::{
    verify_completion, JobOrder, OrderAccepted, OrderCompleted, TOPIC_JOB_ORDER,
};
use simon_node::{Capability, JobResult, RegisterMsg, Registry};

/// Full path fixture: client orders -> node executes -> client verifies.
struct Scena {
    client_key: Keypair,
    node_key: Keypair,
    registry: Registry,
    node_id: String,
}

impl Scena {
    fn nowa() -> Self {
        let client_key = Keypair::generate();
        let node_key = Keypair::generate();
        let mut registry = Registry::new();
        let node_id = "node-1".to_string();

        let cap = Capability {
            declared: serde_json::json!({"vram_gb": 12, "tok_s": 45}),
            attested: Some(serde_json::json!({"tok_s": 42.1})),
        };
        let msg = RegisterMsg::new(node_id.clone(), node_key.public(), cap);
        assert!(registry.register(msg), "node registration");

        Self {
            client_key,
            node_key,
            registry,
            node_id,
        }
    }

    fn zlecenie(&self, _order_id: &str, fee: u64) -> JobOrder {
        // order_id computed from digest + nonce.
        let mut o = JobOrder::new(
            "sha256:qwen3.8-27b-w4a16",
            b"compute something",
            fee,
            300,
            self.client_key.public().to_hex(),
        )
        .expect("order");
        let digest = o.digest().expect("digest");
        o.signature = Some(hex::encode(
            self.client_key.sign_digest(&digest).0,
        ));
        o
    }

    /// Node completes work and submits signed receipt.
    fn wykonaj(&mut self, order: &JobOrder) -> JobResult {
        let job_id = format!("job-{}", order.order_id);
        assert!(
            self.registry.assign(&job_id, &self.node_id),
            "job assignment"
        );

        let receipt = Receipt {
            job_id: job_id.clone(),
            node_id: self.node_id.clone(),
            model_hash: order.model_hash.clone(),
            runtime: "vllm-0.6.3".to_string(),
            precision: Precision::Bf16,
            activation_hash: "toploc:258B".to_string(),
            output_digest: "sha256:wynik".to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            started_at_us: 100_000_000,
            finished_at_us: 100_400_000,
            signer: self.node_key.public(),
            signature: None,
            bindings: None,
        }
        .sign(&self.node_key)
        .expect("receipt signature");

        let result = JobResult::success(
            job_id,
            self.node_id.clone(),
            serde_json::json!({"tokens": 128}),
            receipt,
        );
        self.registry.complete(&result).expect("result accepted");
        result
    }

    /// Client verifies result directly using registered public key.
    fn klient_weryfikuje(
        &self,
        wiazanie: &str,
        oczekiwane: &str,
        result: &JobResult,
    ) -> Result<(), SimonError> {
        let receipt = result.receipt.as_ref().ok_or(SimonError::BadSignature)?;
        let registered = self
            .registry
            .nodes
            .get(&self.node_id)
            .expect("node in registry")
            .pubkey
            .clone();

        let completed = OrderCompleted {
            order_id: wiazanie.to_string(),
            job_id: receipt.job_id.clone(),
            node_id: self.node_id.clone(),
            receipt: receipt.clone(),
            coordinator_pubkey: "coord-1".to_string(),
        };

        verify_completion(&completed, oczekiwane, &registered)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn pelny_cykl_przechodzi() {
    let mut s = Scena::nowa();
    let order = s.zlecenie("o-1", 100);
    let result = s.wykonaj(&order);
    assert!(
        s.klient_weryfikuje(&order.order_id, &order.order_id, &result).is_ok(),
        "client must verify result autonomously"
    );
}

#[test]
fn zlecenie_bez_oplaty_nie_generuje_emisji() {
    let s = Scena::nowa();
    let order = s.zlecenie("o-2", 0);
    assert!(!order.is_funded(), "fee_think == 0 -> no emissions");
}

#[test]
fn roboczy_burn_proof_nie_jest_produkcyjny() {
    let s = Scena::nowa();
    let order = s.zlecenie("o-3", 50);
    assert!(order.burn.synthetic);
    assert!(!order.has_verifiable_burn(), "MVP does not claim production burn");
}

#[test]
fn klient_odrzuca_wynik_dla_innego_zlecenia() {
    let mut s = Scena::nowa();
    let order = s.zlecenie("o-4", 50);
    let result = s.wykonaj(&order);

    assert!(
        s.klient_weryfikuje("o-OTHER", &order.order_id, &result).is_err(),
        "result for another order must be rejected"
    );

    assert!(
        s.klient_weryfikuje(&order.order_id, &order.order_id, &result).is_ok(),
        "sanity: matching order must pass"
    );
}

#[test]
fn klient_odrzuca_receipt_podpisany_obcym_kluczem() {
    let mut s = Scena::nowa();
    let order = s.zlecenie("o-5", 50);

    let job_id = format!("job-{}", order.order_id);
    s.registry.assign(&job_id, &s.node_id);

    let obcy = Keypair::generate();
    let receipt = Receipt {
        job_id: job_id.clone(),
        node_id: s.node_id.clone(),
        model_hash: order.model_hash.clone(),
        runtime: "vllm-0.6.3".to_string(),
        precision: Precision::Bf16,
        activation_hash: "toploc:258B".to_string(),
        output_digest: "sha256:wynik".to_string(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 100_000_000,
        finished_at_us: 100_400_000,
        signer: obcy.public(),
        signature: None,
        bindings: None,
    }
    .sign(&obcy)
    .unwrap();

    let result = JobResult::success(
        job_id,
        s.node_id.clone(),
        serde_json::json!({}),
        receipt,
    );

    assert!(
        s.klient_weryfikuje(&order.order_id, &order.order_id, &result).is_err(),
        "foreign key must be rejected"
    );
}

#[test]
fn klient_odrzuca_podmieniona_tresc_receiptu() {
    let mut s = Scena::nowa();
    let order = s.zlecenie("o-6", 50);
    let mut result = s.wykonaj(&order);

    if let Some(r) = result.receipt.as_mut() {
        r.activation_hash = "toploc:FAKE".to_string();
    }

    assert!(
        s.klient_weryfikuje(&order.order_id, &order.order_id, &result).is_err(),
        "tampered receipt content must be detected"
    );
}

#[test]
fn wyscig_gossipsub_rozstrzygany_deterministycznie() {
    let mk = |pk: &str| OrderAccepted {
        order_id: "o-7".into(),
        job_id: "job-o-7".into(),
        coordinator_pubkey: pk.to_string(),
        vrf_seed: "vrf:1".into(),
        vrf_block: 42,
        estimated_ms: 200,
        signature: None,
    };

    let k1 = mk("0a1b");
    let k2 = mk("ff99");
    let k3 = mk("5c3d");
    let kandydaci = [k1, k2, k3];

    let zwyciezca = OrderAccepted::winner(&kandydaci).expect("winner present");
    let ranking = |pk: &str| {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"o-7|");
        h.update(pk.as_bytes());
        hex::encode(h.finalize())
    };
    let oczekiwany = ["0a1b", "ff99", "5c3d"]
        .into_iter()
        .min_by_key(|pk| ranking(pk))
        .unwrap();
    assert_eq!(zwyciezca.coordinator_pubkey, oczekiwany, "winner is min H(order_id || pubkey)");

    for i in 0..kandydaci.len() {
        let mut obrocone = kandydaci.clone();
        obrocone.rotate_left(i);
        let w = OrderAccepted::winner(&obrocone).unwrap();
        assert_eq!(
            w.coordinator_pubkey, oczekiwany,
            "response order must not alter deterministic winner"
        );
    }
}

#[test]
fn tematy_gossipsub_sa_czescia_kontraktu() {
    assert_eq!(TOPIC_JOB_ORDER, "simon/v1/job-order");
}

#[test]
fn porazka_nie_wymaga_receiptu() {
    let mut s = Scena::nowa();
    let job_id = "job-fail";
    s.registry.assign(job_id, &s.node_id);
    let result = JobResult::failure(job_id, s.node_id.clone(), "OOM");
    assert!(s.registry.complete(&result).is_ok());
}

#[test]
fn typy_sa_serializowalne_do_json() {
    let s = Scena::nowa();
    let order = s.zlecenie("o-json", 10);

    let json = serde_json::to_string(&order).expect("JobOrder -> JSON");
    let back: JobOrder = serde_json::from_str(&json).expect("JSON -> JobOrder");
    assert_eq!(back.order_id, order.order_id);
    assert_eq!(back.digest().unwrap(), order.digest().unwrap());

    assert_eq!(
        back.payload_digest, order.payload_digest,
        "round-trip must not alter payload digest"
    );
}
