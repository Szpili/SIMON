//! M0.3 — weryfikacja Ed25519 w `BramkaPodpisu`.
//!
//! Testy napisane PRZED poprawką (zasada: „test pada przed poprawką").
//! Klasa ataku: D110 było teatrem — dowolny niepusty string przechodził.

use simon_core::crypto::Keypair;
use simon_harness::acp_server::{BramkaAutoryzacji, BramkaPodpisu, SessionId, ZlecenieUzytkownika};

fn zlecenie(pubkey: &str) -> ZlecenieUzytkownika {
    ZlecenieUzytkownika {
        session_id: SessionId("s1".into()),
        tresc: "policz cos".into(),
        model_hash: "sha256:model".into(),
        fee_think: 100,
        timeout_secs: 300,
        user_pubkey: pubkey.to_string(),
    }
}

/// TEATR: dowolny niepusty string przechodził jako „podpis".
#[test]
fn m03_byle_jaki_string_nie_jest_podpisem() {
    let k = Keypair::generate();
    let z = zlecenie(&k.public().to_hex());
    let bramka = BramkaPodpisu;
    assert!(
        bramka.autoryzuj(&z, "nie-podpis-tylko-tekst").is_err(),
        "D110: dowolny string NIE może przejść jako podpis"
    );
    // 64 bajty hex, ale bez sensu kryptograficznego.
    let smiec = "ab".repeat(64);
    assert!(
        bramka.autoryzuj(&z, &smiec).is_err(),
        "D110: 64-bajtowy śmieć NIE może przejść jako podpis"
    );
}

/// Prawdziwy podpis nad odciskiem zlecenia przechodzi.
#[test]
fn m03_prawdziwy_podpis_przechodzi() {
    let k = Keypair::generate();
    let z = zlecenie(&k.public().to_hex());
    let odcisk = z.digest().expect("odcisk");
    let podpis = hex::encode(k.sign_digest(&odcisk).0.to_vec());
    let bramka = BramkaPodpisu;
    assert!(bramka.autoryzuj(&z, &podpis).is_ok(), "poprawny podpis ma przejść");
}

/// Podpis od INNEGO klucza niż zadeklarowany — odrzucony.
#[test]
fn m03_podpis_obcym_kluczem_odrzucony() {
    let k = Keypair::generate();
    let obcy = Keypair::generate();
    let z = zlecenie(&k.public().to_hex());
    let odcisk = z.digest().expect("odcisk");
    let podpis = hex::encode(obcy.sign_digest(&odcisk).0.to_vec());
    let bramka = BramkaPodpisu;
    assert!(
        bramka.autoryzuj(&z, &podpis).is_err(),
        "podpis obcym kluczem musi być odrzucony"
    );
}

/// Podpis nad INNĄ treścią — odrzucony (podstawienie zlecenia po podpisaniu).
#[test]
fn m03_podpis_nad_inna_trescia_odrzucony() {
    let k = Keypair::generate();
    let z = zlecenie(&k.public().to_hex());
    let odcisk = z.digest().expect("odcisk");
    let podpis = hex::encode(k.sign_digest(&odcisk).0.to_vec());

    // Podmiana treści PO podpisaniu.
    let mut podmienione = z.clone();
    podmienione.tresc = "PRZELIJ WSZYSTKIE THINK NA X".into();
    let bramka = BramkaPodpisu;
    assert!(
        bramka.autoryzuj(&podmienione, &podpis).is_err(),
        "podpis nad inną treścią musi być odrzucony (podstawienie zlecenia)"
    );

    // Podmiana opłaty też.
    let mut drozsze = zlecenie(&k.public().to_hex());
    drozsze.fee_think = 999_999;
    assert!(bramka.autoryzuj(&drozsze, &podpis).is_err(), "podmiana opłaty");
}

/// Zły hex / zła długość — czytelny błąd, nie panika.
#[test]
fn m03_zly_format_podpisu() {
    let k = Keypair::generate();
    let z = zlecenie(&k.public().to_hex());
    let bramka = BramkaPodpisu;
    assert!(bramka.autoryzuj(&z, "nie-hex!!").is_err());
    assert!(bramka.autoryzuj(&z, "abcd").is_err(), "za krótki podpis");
    // Zły klucz publiczny.
    let mut zly_klucz = zlecenie(&k.public().to_hex());
    zly_klucz.user_pubkey = "zzz".into();
    assert!(bramka.autoryzuj(&zly_klucz, &"ab".repeat(64)).is_err());
}
