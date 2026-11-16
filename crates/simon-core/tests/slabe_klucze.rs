//! Regression (2026-10-31): weak public keys must not yield valid signatures.
//!
//! Root cause fixed: PublicKey::verify_digest now calls verify_strict and checks
//! key.is_weak(), rejecting small-order points (e.g. identity point 01||00*31).

use simon_core::crypto::{Keypair, PublicKey, Signature};

/// 256 different digests to verify rejection across edge cases.
fn odciski() -> impl Iterator<Item = String> {
    (0u32..256).map(|i| format!("{i:064x}"))
}

fn ile_przechodzi(klucz: [u8; 32], podpis: [u8; 64]) -> usize {
    odciski()
        .filter(|d| {
            PublicKey(klucz)
                .verify_digest(d, &Signature(podpis))
                .is_ok()
        })
        .count()
}

#[test]
fn klucz_tozsamosciowy_nie_podrabia_podpisu() {
    let mut klucz = [0u8; 32];
    klucz[0] = 1; // 01||00*31 = identity point (order 1)
    let mut podpis = [0u8; 64];
    podpis[0] = 1; // R = identity, s = 0
    assert_eq!(
        ile_przechodzi(klucz, podpis),
        0,
        "digests accepted with identity key"
    );
}

#[test]
fn klucz_zerowy_nie_podrabia_podpisu() {
    assert_eq!(
        ile_przechodzi([0u8; 32], [0u8; 64]),
        0,
        "digests accepted with 00..00 key"
    );
}

#[test]
fn zwykly_klucz_dalej_dziala() {
    let k = Keypair::from_seed(&[7u8; 32]);
    for d in odciski().take(8) {
        let s = k.sign_digest(&d);
        assert!(k.public().verify_digest(&d, &s).is_ok());
        assert!(k.public().verify_digest(&format!("{d}0"), &s).is_err());
    }
}
