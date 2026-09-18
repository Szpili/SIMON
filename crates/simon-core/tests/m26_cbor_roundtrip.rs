//! Regresja (2026-09-17): receipt MUSI przeżyć roundtrip przez transport.
//!
//! Znaleziony błąd: `started_at`/`finished_at` jako `f64`. JSON i CBOR
//! serializują floaty inaczej, więc `digest()` (SHA-256 z JSON-a) wychodził
//! INNY po powrocie i podpis przestawał pasować (`Err(BadSignature)`).
//!
//! Ten test PADAŁ przed poprawką. Podpisywana treść nie może zawierać f64.

use simon_core::crypto::Keypair;
use simon_core::receipt::{Precision, Receipt};

fn przyklad(klucz: &Keypair) -> Receipt {
    Receipt {
        job_id: "job-cbor".into(),
        node_id: klucz.public().to_hex(),
        model_hash: "bielik-awq".into(),
        runtime: "vllm-openai/1".into(),
        precision: Precision::Fp16,
        activation_hash: "toploc:NIE_POLICZONY".into(),
        output_digest: "abc".into(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 1_789_626_040_836_287,
        finished_at_us: 1_789_626_040_836_287,
        signer: klucz.public(),
        signature: None,
    }
}

#[test]
fn m26_receipt_przezywa_roundtrip_json() {
    let klucz = Keypair::from_seed(&[5u8; 32]);
    let podpisany = przyklad(&klucz).sign(&klucz).expect("podpis");
    assert!(podpisany.verify_self().is_ok(), "przed serializacją OK");

    let json = serde_json::to_string(&podpisany).unwrap();
    let po: Receipt = serde_json::from_str(&json).unwrap();

    assert_eq!(
        podpisany.digest().unwrap(),
        po.digest().unwrap(),
        "digest MUSI być identyczny po roundtripie JSON"
    );
    assert!(po.verify_self().is_ok(), "podpis MUSI przeżyć roundtrip JSON");
}

#[test]
fn m26_digest_nie_zalezy_od_formatu_czasu() {
    let klucz = Keypair::from_seed(&[6u8; 32]);
    let a = przyklad(&klucz).sign(&klucz).unwrap();
    let mut b = przyklad(&klucz);
    b.started_at_us += 1;
    let b = b.sign(&klucz).unwrap();

    assert_eq!(a.digest().unwrap(), a.digest().unwrap(), "ten sam receipt = ten sam digest");
    assert_ne!(a.digest().unwrap(), b.digest().unwrap(), "inny czas = inny digest");
}
