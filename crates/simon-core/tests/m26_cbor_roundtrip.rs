//! Regression (2026-09-17): receipt must survive roundtrip through transport.
//!
//! Root cause fixed: integer timestamps `started_at_us`/`finished_at_us` avoid float
//! differences across serializers (JSON/CBOR). Signed content must not contain f64.

use simon_core::crypto::Keypair;
use simon_core::receipt::{Precision, Receipt};

fn przyklad(klucz: &Keypair) -> Receipt {
    Receipt {
        job_id: "job-cbor".into(),
        node_id: klucz.public().to_hex(),
        model_hash: "bielik-awq".into(),
        runtime: "vllm-openai/1".into(),
        precision: Precision::Fp16,
        activation_hash: "toploc:NOT_COMPUTED".into(),
        output_digest: "abc".into(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 1_789_626_040_836_287,
        finished_at_us: 1_789_626_040_836_287,
        signer: klucz.public(),
        signature: None,
        bindings: None,
    }
}

#[test]
fn m26_receipt_przezywa_roundtrip_json() {
    let klucz = Keypair::from_seed(&[5u8; 32]);
    let podpisany = przyklad(&klucz).sign(&klucz).expect("signature");
    assert!(podpisany.verify_self().is_ok(), "before serialization OK");

    let json = serde_json::to_string(&podpisany).unwrap();
    let po: Receipt = serde_json::from_str(&json).unwrap();

    assert_eq!(
        podpisany.digest().unwrap(),
        po.digest().unwrap(),
        "digest must be identical after JSON roundtrip"
    );
    assert!(po.verify_self().is_ok(), "signature must survive JSON roundtrip");
}

#[test]
fn m26_digest_nie_zalezy_od_formatu_czasu() {
    let klucz = Keypair::from_seed(&[6u8; 32]);
    let a = przyklad(&klucz).sign(&klucz).unwrap();
    let mut b = przyklad(&klucz);
    b.started_at_us += 1;
    let b = b.sign(&klucz).unwrap();

    assert_eq!(a.digest().unwrap(), a.digest().unwrap(), "same receipt = same digest");
    assert_ne!(a.digest().unwrap(), b.digest().unwrap(), "different time = different digest");
}
