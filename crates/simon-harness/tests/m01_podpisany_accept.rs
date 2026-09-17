//! M0.1 — podpisane `OrderAccepted` + filtr PRZED `winner()`.
//!
//! Testy napisane PRZED poprawką (zasada: „test pada przed poprawką").
//! Klasa ataku (macierz RT3): przejęcie koordynatora przez wygenerowany klucz.

use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{OrderAccepted, RejestrKoordynatorow};

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

/// Atakujący generuje klucz o hashu LEPSZYM niż uczciwy, ale NIE MA go na liście.
#[test]
fn m01_niepodpisany_kandydat_przegrywa_z_uczciwym() {
    let uczciwy_klucz = Keypair::generate();
    let uczciwy = accept(&uczciwy_klucz.public().to_hex(), "job-uczciwy");
    let uczciwy = uczciwy.sign(&uczciwy_klucz);

    // Niezarejestrowany atakujący — bez podpisu, ale z kluczem "0000...".
    let atak = accept(
        "0000000000000000000000000000000000000000000000000000000000000000",
        "job-atak",
    );

    let rejestr = RejestrKoordynatorow::z_kluczami(&[uczciwy_klucz.public().to_hex()]);
    let kandydaci = [atak.clone(), uczciwy.clone()];

    // Filtrowanie PRZED rankingiem: niepodpisany/niezarejestrowany wypada.
    let dopuszczeni = rejestr.dopusc(&kandydaci, "o1");
    assert_eq!(dopuszczeni.len(), 1, "niezarejestrowany kandydat musi wypaść");

    let zwyciezca = rejestr.winner_zrejestrowany(&kandydaci, "o1").expect("zwycięzca");
    assert_eq!(
        zwyciezca.coordinator_pubkey,
        uczciwy_klucz.public().to_hex(),
        "wygrać ma URGZĘDOWY uczciwy, nie atakujący z ładniejszym hashem"
    );
}

/// Zarejestrowany, ale bez podpisu — też musi wypaść.
#[test]
fn m01_zarejestrowany_bez_podpisu_wypada() {
    let k = Keypair::generate();
    let bez_podpisu = accept(&k.public().to_hex(), "job-x");
    let rejestr = RejestrKoordynatorow::z_kluczami(&[k.public().to_hex()]);
    let kandydaci = [bez_podpisu];
    let dopuszczeni = rejestr.dopusc(&kandydaci, "o1");
    assert!(dopuszczeni.is_empty(), "zarejestrowany bez podpisu NIE może wejść");
}

/// Podpis od INNEGO klucza niż zadeklarowany — musi wypaść.
#[test]
fn m01_podpis_obcym_kluczem_wypada() {
    let k = Keypair::generate();
    let obcy = Keypair::generate();
    let podszycie = accept(&k.public().to_hex(), "job-x").sign(&obcy);
    let rejestr = RejestrKoordynatorow::z_kluczami(&[k.public().to_hex()]);
    let kandydaci = [podszycie];
    assert!(
        rejestr.dopusc(&kandydaci, "o1").is_empty(),
        "podpis obcym kluczem musi być odrzucony"
    );
}

/// Uczciwy, podpisany kandydat przechodzi i wygrywa.
#[test]
fn m01_uczciwy_przechodzi() {
    let k = Keypair::generate();
    let ok = accept(&k.public().to_hex(), "job-ok").sign(&k);
    let rejestr = RejestrKoordynatorow::z_kluczami(&[k.public().to_hex()]);
    let kandydaci = [ok.clone()];
    assert_eq!(rejestr.dopusc(&kandydaci, "o1").len(), 1);
    assert_eq!(
        rejestr.winner_zrejestrowany(&kandydaci, "o1").unwrap().job_id,
        "job-ok"
    );
}

/// Podpis nad innym order_id nie pasuje do tego zlecenia.
#[test]
fn m01_podpis_nie_odpowiada_zleceniu() {
    let k = Keypair::generate();
    let mut a = accept(&k.public().to_hex(), "job-a").sign(&k);
    a.order_id = "o-INNE".into(); // podmiana po podpisaniu
    let rejestr = RejestrKoordynatorow::z_kluczami(&[k.public().to_hex()]);
    let kandydaci = [a];
    assert!(
        rejestr.dopusc(&kandydaci, "o1").is_empty(),
        "podpis nad innym order_id nie może przejść"
    );
}
