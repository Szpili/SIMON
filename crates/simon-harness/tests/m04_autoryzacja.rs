//! M0.4 — user authorization in `JobOrder`: version, nonce, expiration, coordinator verification.

use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{
    JobOrder, OrderAuthorization, SpentAuthorizationRegistry, CLOCK_TOLERANCE_SECS, FORMAT_V,
};

fn now() -> u64 {
    1_800_000_000
}

/// Authorization consistent with order. `valid_until` in future.
fn authorization_from(order: &JobOrder, keypair: &Keypair, valid_until: u64) -> OrderAuthorization {
    OrderAuthorization {
        v: FORMAT_V,
        payload_digest: order.payload_digest.clone(),
        model_hash: order.model_hash.clone(),
        fee_think: order.fee_think,
        timeout_secs: order.timeout_secs,
        nonce: order.nonce.clone(),
        valid_until,
        user_pubkey: keypair.public().to_hex(),
        signature: None,
    }
    .sign(keypair)
}

fn signed_order(fee: u64) -> (JobOrder, Keypair) {
    let k = Keypair::generate();
    let mut order =
        JobOrder::new("sha256:qwen", b"compute this", fee, 300, k.public().to_hex()).expect("order");
    let a = authorization_from(&order, &k, now() + 600);
    order.authorization = Some(a);
    (order, k)
}

#[test]
fn m04_poprawna_autoryzacja_przechodzi() {
    let (order, _k) = signed_order(100);
    assert!(
        order.verify_authorization(now()).is_ok(),
        "valid authorization must pass"
    );
}

#[test]
fn m04_podmiana_oplaty_odrzucona() {
    let (mut order, _k) = signed_order(100);
    order.fee_think = 999_999;
    assert!(
        order.verify_authorization(now()).is_err(),
        "fee substitution must be rejected"
    );
}

#[test]
fn m04_podmiana_modelu_odrzucona() {
    let (mut order, _k) = signed_order(100);
    order.model_hash = "sha256:other-model".into();
    assert!(order.verify_authorization(now()).is_err(), "model substitution");
}

#[test]
fn m04_podmiana_tresci_odrzucona() {
    let (mut order, _k) = signed_order(100);
    order.payload_digest = "deadbeef".repeat(8);
    assert!(order.verify_authorization(now()).is_err(), "content substitution");
}

#[test]
fn m04_przeterminowana_odrzucona() {
    let k = Keypair::generate();
    let mut order = JobOrder::new("sha256:m", b"x", 100, 300, k.public().to_hex()).unwrap();
    let a = authorization_from(&order, &k, now() - CLOCK_TOLERANCE_SECS - 10);
    order.authorization = Some(a);
    assert!(
        order.verify_authorization(now()).is_err(),
        "expired authorization must be rejected"
    );
    let mut ok = JobOrder::new("sha256:m", b"x", 100, 300, k.public().to_hex()).unwrap();
    let a2 = authorization_from(&ok, &k, now() - CLOCK_TOLERANCE_SECS + 5);
    ok.authorization = Some(a2);
    assert!(ok.verify_authorization(now()).is_ok(), "clock tolerance");
}

#[test]
fn m04_cudzy_klucz_odrzucony() {
    let (mut order, _k) = signed_order(100);
    let foreign = Keypair::generate();
    order.client_pubkey = foreign.public().to_hex();
    assert!(order.verify_authorization(now()).is_err(), "foreign key");
}

#[test]
fn m04_brak_autoryzacji_odrzucony() {
    let k = Keypair::generate();
    let order = JobOrder::new("sha256:m", b"x", 100, 300, k.public().to_hex()).unwrap();
    assert!(order.authorization.is_none(), "fresh order has no authorization");
    assert!(
        order.verify_authorization(now()).is_err(),
        "D110: coordinator rejects order without authorization"
    );
}

#[test]
fn m04_ta_sama_autoryzacja_dwa_razy_odrzucona() {
    let (order, _k) = signed_order(100);
    let digest = order.verify_authorization(now()).expect("first");
    let mut registry = SpentAuthorizationRegistry::default();
    registry.consume(&digest).expect("first consume");
    assert!(
        registry.consume(&digest).is_err(),
        "duplicate authorization must be rejected"
    );
}

#[test]
fn m04_nieznana_wersja_odrzucona() {
    let (mut order, _k) = signed_order(100);
    if let Some(a) = order.authorization.as_mut() {
        a.v = 999;
    }
    assert!(order.verify_authorization(now()).is_err(), "unknown version");
}

#[test]
fn m04_nonce_w_haszu_tresci() {
    let k = Keypair::generate();
    let a = JobOrder::new("sha256:m", b"yes", 1, 1, k.public().to_hex()).unwrap();
    let b = JobOrder::new("sha256:m", b"yes", 1, 1, k.public().to_hex()).unwrap();
    assert_ne!(
        a.payload_digest, b.payload_digest,
        "same short prompt with different nonce gives different digest"
    );
}
