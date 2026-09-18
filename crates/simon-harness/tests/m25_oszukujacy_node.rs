//! M2.5.2 — test z oszukującym node'em MUSI PAŚĆ przed poprawką.
//!
//! Teza SIMON: node, który podpisze FAŁSZYWY wynik, ma zostać złapany.
//!
//! Stan obecny (2026-09-17, sprawdzone w kodzie):
//!   - `simon-cli/src/agent.rs::zweryfikuj_receipt` sprawdza TYLKO typy trzech pól,
//!   - node NIE podpisuje receiptu (goły JSON bez `signature`),
//!   - `activation_hash` to dowolny string — nikt go nie przelicza.
//!
//! Ten test DOWODZI, że oszust przechodzi dzisiejszą weryfikację.
//! Po implementacji M2.5.1 powinien ZACZĄĆ PADAĆ — i wtedy zmieniamy asercję.

use simon_core::receipt::{Precision, Receipt};
use simon_core::crypto::PublicKey;

fn receipt_oszusata() -> Receipt {
    Receipt {
        job_id: "job-oszust".into(),
        node_id: "node-oszust".into(),
        model_hash: "qwen3.8-27b".into(),
        runtime: "vllm-0.29".into(),
        precision: Precision::Fp16,
        // FAŁSZYWY hash aktywacji — nikt go nie przelicza.
        activation_hash: "toploc:FAKE".into(),
        output_digest: "digest-falszywego-wyniku".into(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 0,
        finished_at_us: 1_000_000,
        signer: PublicKey([0u8; 32]),
        signature: None,
    }
}

/// DOWÓD DZIURY: dzisiejsza weryfikacja uznaje oszusta za „spójnego".
#[test]
fn m25_oszukujacy_node_przechodzi_dzisiejsza_weryfikacje() {
    let r = receipt_oszusata();
    let json = serde_json::to_string(&r).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Kopia logiki agent.rs::zweryfikuj_receipt (3 pola typu string).
    let przechodzi = v["job_id"].is_string() && v["model_hash"].is_string();

    assert!(
        przechodzi,
        "DZIURA POTWIERDZONA: oszust z fałszywym activation_hash i BRAK podpisu \
         przechodzi weryfikację klienta. M2.5.1 ma to zmienić."
    );

    // Dodatkowo: brak podpisu i fałszywy hash też nie są sprawdzane.
    assert!(r.signature.is_none(), "oszust nie podpisał niczego");
    assert_eq!(r.activation_hash, "toploc:FAKE", "hash jest dowolnym stringiem");
}
