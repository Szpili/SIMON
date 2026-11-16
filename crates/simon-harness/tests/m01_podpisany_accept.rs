//! M0.1 — Signed `OrderAccepted` and filter before `winner()`.
//!
//! Attack class (RT3 matrix): hijacking the coordinator via generated untrusted keys.

use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{CoordinatorRegistry, OrderAccepted};

fn accept(pk: &str, job: &str) -> OrderAccepted {
    OrderAccepted {
        order_id: "o1".into(),
        job_id: job.into(),
        coordinator_pubkey: pk.into(),
        vrf_seed: "seed".into(),
        vrf_block: 7,
        estimated_ms: 100,
        signature: None,
    }
}

/// Attacker generates a key with a lower hash, but is not in registry.
#[test]
fn m01_unsigned_candidate_loses_to_honest() {
    let honest_key = Keypair::generate();
    let honest = accept(&honest_key.public().to_hex(), "job-honest");
    let honest = honest.sign(&honest_key);

    // Unregistered attacker without signature
    let attack = accept(
        "0000000000000000000000000000000000000000000000000000000000000000",
        "job-attack",
    );

    let registry = CoordinatorRegistry::with_keys(&[honest_key.public().to_hex()]);
    let candidates = [attack.clone(), honest.clone()];

    // Filtering before ranking: unsigned/unregistered is dropped.
    let admitted = registry.admit(&candidates, "o1");
    assert_eq!(admitted.len(), 1, "unregistered candidate must be dropped");

    let winner = registry.winner_registered(&candidates, "o1").expect("winner");
    assert_eq!(
        winner.coordinator_pubkey,
        honest_key.public().to_hex(),
        "registered honest candidate must win over unverified candidate"
    );
}

/// Registered coordinator without signature must also be dropped.
#[test]
fn m01_registered_without_signature_is_dropped() {
    let k = Keypair::generate();
    let unsigned = accept(&k.public().to_hex(), "job-x");
    let registry = CoordinatorRegistry::with_keys(&[k.public().to_hex()]);
    let candidates = [unsigned];
    let admitted = registry.admit(&candidates, "o1");
    assert!(admitted.is_empty(), "registered without signature cannot be admitted");
}

/// Signature from key other than declared must be rejected.
#[test]
fn m01_signature_from_wrong_key_is_rejected() {
    let k = Keypair::generate();
    let other = Keypair::generate();
    let spoof = accept(&k.public().to_hex(), "job-x").sign(&other);
    let registry = CoordinatorRegistry::with_keys(&[k.public().to_hex()]);
    let candidates = [spoof];
    assert!(
        registry.admit(&candidates, "o1").is_empty(),
        "signature from wrong key must be rejected"
    );
}

/// Honest signed candidate passes and wins.
#[test]
fn m01_honest_passes() {
    let k = Keypair::generate();
    let ok = accept(&k.public().to_hex(), "job-ok").sign(&k);
    let registry = CoordinatorRegistry::with_keys(&[k.public().to_hex()]);
    let candidates = [ok.clone()];
    assert_eq!(registry.admit(&candidates, "o1").len(), 1);
    assert_eq!(
        registry.winner_registered(&candidates, "o1").unwrap().job_id,
        "job-ok"
    );
}

/// Signature over different order_id does not match this order.
#[test]
fn m01_signature_mismatched_order_id() {
    let k = Keypair::generate();
    let mut a = accept(&k.public().to_hex(), "job-a").sign(&k);
    a.order_id = "o-OTHER".into(); // tamper after signing
    let registry = CoordinatorRegistry::with_keys(&[k.public().to_hex()]);
    let candidates = [a];
    assert!(
        registry.admit(&candidates, "o1").is_empty(),
        "signature over different order_id must be rejected"
    );
}
