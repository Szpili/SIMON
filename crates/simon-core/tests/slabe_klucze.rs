//! Regresja (2026-10-31): słaby klucz publiczny nie może dawać ważnego podpisu.
//!
//! Znaleziony błąd: `PublicKey::verify_digest` wołał `verify` z ed25519-dalek,
//! który nie odrzuca punktów małego rzędu. Klucz `01||00*31` (punkt
//! tożsamościowy) z podpisem R = tożsamość, s = 0 przechodził dla KAŻDEGO
//! odcisku, więc każdy mógł „podpisać” dowolny receipt bez klucza prywatnego.
//!
//! Dwa pierwsze testy PADAŁY przed poprawką (`is_weak` + `verify_strict`):
//! przyjęte 256/256 i 67/256 odcisków. Trzeci to kontrola zwykłego klucza.

use simon_core::crypto::{Keypair, PublicKey, Signature};

/// 256 różnych odcisków. Dla klucza 00..00 (rząd 4) wynik zależy od
/// (SHA-512(R‖A‖M) mod ℓ) mod 4, więc pojedynczy odcisk niczego nie dowodzi;
/// dla punktu tożsamościowego wynik od odcisku nie zależy.
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
    klucz[0] = 1; // 01||00*31 = punkt tożsamościowy (rząd 1)
    let mut podpis = [0u8; 64];
    podpis[0] = 1; // R = tożsamość, s = 0
    assert_eq!(
        ile_przechodzi(klucz, podpis),
        0,
        "odciski przyjęte z kluczem tożsamościowym"
    );
}

#[test]
fn klucz_zerowy_nie_podrabia_podpisu() {
    assert_eq!(
        ile_przechodzi([0u8; 32], [0u8; 64]),
        0,
        "odciski przyjęte z kluczem 00..00"
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
