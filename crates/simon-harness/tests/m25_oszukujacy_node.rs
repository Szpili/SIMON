//! M2.5.2 - test documenting legacy verification gap before signature requirement.

use simon_core::receipt::{Precision, Receipt};
use simon_core::crypto::PublicKey;

fn receipt_oszusata() -> Receipt {
    Receipt {
        job_id: "job-oszust".into(),
        node_id: "node-oszust".into(),
        model_hash: "qwen3.8-27b".into(),
        runtime: "vllm-0.29".into(),
        precision: Precision::Fp16,
        activation_hash: "toploc:FAKE".into(),
        output_digest: "digest-falszywego-wyniku".into(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 0,
        finished_at_us: 1_000_000,
        signer: PublicKey([0u8; 32]),
        signature: None,
        bindings: None,
    }
}

/// Baseline test: legacy superficial check accepted unsigned receipt with arbitrary activation hash.
#[test]
fn m25_oszukujacy_node_przechodzi_dzisiejsza_weryfikacje() {
    let r = receipt_oszusata();
    let json = serde_json::to_string(&r).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Copy of verification logic from agent.rs::verify_receipt.
    let przechodzi = v["job_id"].is_string() && v["model_hash"].is_string();

    assert!(
        przechodzi,
        "historical verification check passed unsigned receipts without signature"
    );

    assert!(r.signature.is_none(), "no signature present");
    assert_eq!(r.activation_hash, "toploc:FAKE", "hash is arbitrary string");
}
