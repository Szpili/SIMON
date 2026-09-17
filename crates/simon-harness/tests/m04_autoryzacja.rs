//! M0.4 — autoryzacja użytkownika w `JobOrder`: wersja, nonce, termin, weryfikacja
//! po stronie KOORDYNATORA.
//!
//! Testy napisane PRZED poprawką (zasada: „test pada przed poprawką").
//! Klasa ataku (macierz RT3): przejęty agent / harness (D110) + podstawienie zlecenia.

use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{
    Autoryzacja, JobOrder, RejestrZuzytychAutoryzacji, FORMAT_V, TOLERANCJA_ZEGARA_SECS,
};

fn teraz() -> u64 {
    1_800_000_000
}

/// Autoryzacja spójna ze zleceniem. `wazne_do` w przyszłości.
fn autoryzacja_z(order: &JobOrder, klucz: &Keypair, wazne_do: u64) -> Autoryzacja {
    Autoryzacja {
        v: FORMAT_V,
        payload_digest: order.payload_digest.clone(),
        model_hash: order.model_hash.clone(),
        fee_think: order.fee_think,
        timeout_secs: order.timeout_secs,
        nonce: order.nonce.clone(),
        wazne_do,
        user_pubkey: klucz.public().to_hex(),
        signature: None,
    }
    .sign(klucz)
}

fn zlecenie_podpisane(fee: u64) -> (JobOrder, Keypair) {
    let k = Keypair::generate();
    let mut order =
        JobOrder::new("sha256:qwen", b"policz to", fee, 300, k.public().to_hex()).expect("order");
    let a = autoryzacja_z(&order, &k, teraz() + 600);
    order.autoryzacja = Some(a);
    (order, k)
}

/// Kontrola: poprawna autoryzacja przechodzi.
#[test]
fn m04_poprawna_autoryzacja_przechodzi() {
    let (order, _k) = zlecenie_podpisane(100);
    assert!(
        order.zweryfikuj_autoryzacje(teraz()).is_ok(),
        "poprawna autoryzacja musi przejść"
    );
}

/// Podstawienie opłaty w `JobOrder` przy starej autoryzacji → odrzucone.
#[test]
fn m04_podmiana_oplaty_odrzucona() {
    let (mut order, _k) = zlecenie_podpisane(100);
    order.fee_think = 999_999; // atakujący chce zapłacić cudzym podpisem mniej/więcej
    assert!(
        order.zweryfikuj_autoryzacje(teraz()).is_err(),
        "podmiana opłaty musi być odrzucona"
    );
}

/// Podstawienie modelu → odrzucone.
#[test]
fn m04_podmiana_modelu_odrzucona() {
    let (mut order, _k) = zlecenie_podpisane(100);
    order.model_hash = "sha256:inny-model".into();
    assert!(order.zweryfikuj_autoryzacje(teraz()).is_err(), "podmiana modelu");
}

/// Podstawienie treści (payload_digest) → odrzucone.
#[test]
fn m04_podmiana_tresci_odrzucona() {
    let (mut order, _k) = zlecenie_podpisane(100);
    order.payload_digest = "deadbeef".repeat(8);
    assert!(order.zweryfikuj_autoryzacje(teraz()).is_err(), "podmiana treści");
}

/// Przeterminowana autoryzacja → odrzucona.
#[test]
fn m04_przeterminowana_odrzucona() {
    let k = Keypair::generate();
    let mut order = JobOrder::new("sha256:m", b"x", 100, 300, k.public().to_hex()).unwrap();
    // ważna 10 s temu, poza tolerancją
    let a = autoryzacja_z(&order, &k, teraz() - TOLERANCJA_ZEGARA_SECS - 10);
    order.autoryzacja = Some(a);
    assert!(
        order.zweryfikuj_autoryzacje(teraz()).is_err(),
        "przeterminowana autoryzacja musi być odrzucona"
    );
    // A w granicach tolerancji — przechodzi (rozjazd zegarów).
    let mut ok = JobOrder::new("sha256:m", b"x", 100, 300, k.public().to_hex()).unwrap();
    let a2 = autoryzacja_z(&ok, &k, teraz() - TOLERANCJA_ZEGARA_SECS + 5);
    ok.autoryzacja = Some(a2);
    assert!(ok.zweryfikuj_autoryzacje(teraz()).is_ok(), "tolerancja zegara");
}

/// Cudzy klucz → odrzucone.
#[test]
fn m04_cudzy_klucz_odrzucony() {
    let (mut order, _k) = zlecenie_podpisane(100);
    let obcy = Keypair::generate();
    order.client_pubkey = obcy.public().to_hex();
    assert!(order.zweryfikuj_autoryzacje(teraz()).is_err(), "cudzy klucz");
}

/// Brak autoryzacji → odrzucone (D110: koordynator NIE dopuszcza bez podpisu).
#[test]
fn m04_brak_autoryzacji_odrzucony() {
    let k = Keypair::generate();
    let order = JobOrder::new("sha256:m", b"x", 100, 300, k.public().to_hex()).unwrap();
    assert!(order.autoryzacja.is_none(), "świeże zlecenie nie ma autoryzacji");
    assert!(
        order.zweryfikuj_autoryzacje(teraz()).is_err(),
        "D110: koordynator odrzuca zlecenie bez autoryzacji"
    );
}

/// Ta sama autoryzacja dwa razy → druga odrzucona (M0.4b).
#[test]
fn m04_ta_sama_autoryzacja_dwa_razy_odrzucona() {
    let (order, _k) = zlecenie_podpisane(100);
    let odcisk = order.zweryfikuj_autoryzacje(teraz()).expect("pierwsza");
    let mut rejestr = RejestrZuzytychAutoryzacji::default();
    rejestr.zajmij(&odcisk).expect("pierwsze zajęcie");
    assert!(
        rejestr.zajmij(&odcisk).is_err(),
        "powtórka tej samej autoryzacji musi być odrzucona"
    );
}

/// Nieznana wersja formatu → odrzucona (M0.4a).
#[test]
fn m04_nieznana_wersja_odrzucona() {
    let (mut order, _k) = zlecenie_podpisane(100);
    if let Some(a) = order.autoryzacja.as_mut() {
        a.v = 999;
    }
    assert!(order.zweryfikuj_autoryzacje(teraz()).is_err(), "nieznana wersja");
}

/// H(nonce ‖ treść) — ten sam prompt z różnym nonce daje różny digest.
#[test]
fn m04_nonce_w_haszu_tresci() {
    let k = Keypair::generate();
    let a = JobOrder::new("sha256:m", b"tak", 1, 1, k.public().to_hex()).unwrap();
    let b = JobOrder::new("sha256:m", b"tak", 1, 1, k.public().to_hex()).unwrap();
    assert_ne!(
        a.payload_digest, b.payload_digest,
        "ten sam krótki prompt z różnym nonce = różny digest (blokada słownikowa)"
    );
}
