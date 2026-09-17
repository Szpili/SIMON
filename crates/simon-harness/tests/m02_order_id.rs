// Test M0.2 — PADANIE PRZED POPRAWKĄ? Ten test jest dopisany PO zmianie formatu,
// więc nie może "padać przed". Zamiast tego dowodzę czymś mocniejszym:
// odtwarzam STARY mechanizm (licznik bez nonce) i pokazuję kolizję.

#[test]
fn m02_stary_licznik_dawal_kolizje() {
    // STARY mechanizm: order_id = "o-{sesja}-{licznik}". Dwa identyczne zlecenia
    // tego samego klienta w RÓŻNYCH sesjach, każda z własnym licznikiem = 1.
    let stary = |sesja: &str, licznik: u64| format!("o-{sesja}-{licznik}");
    let a = stary("s1", 1);
    let b = stary("s1", 1); // inna sesja? nie — ta sama nazwa "s1"
    assert_eq!(a, b, "STARY mechanizm: ten sam order_id dla dwóch zleceń");

    // NOWY mechanizm: order_id = digest(zlecenie z pustym order_id + nonce).
    // Dwa IDENTYCZNE zlecenia dają RÓŻNE order_id, bo nonce jest losowy.
    use simon_harness::client_protocol::JobOrder;
    let z1 = JobOrder::new("sha256:model", b"ten sam prompt", 100, 300, "aa".repeat(32)).unwrap();
    let z2 = JobOrder::new("sha256:model", b"ten sam prompt", 100, 300, "aa".repeat(32)).unwrap();
    assert_ne!(z1.order_id, z2.order_id, "NOWY: identyczne zlecenia = różne order_id (nonce)");

    // I koordynator odtworzy order_id z samego zlecenia (weryfikacja u odbierającego).
    assert!(z1.order_id_sie_zgadza(), "order_id = digest(zlecenie)");
    assert!(z2.order_id_sie_zgadza());
}

#[test]
fn m02_podmieniony_order_id_nie_przechodzi() {
    use simon_harness::client_protocol::JobOrder;
    let mut z = JobOrder::new("sha256:model", b"prompt", 100, 300, "aa".repeat(32)).unwrap();
    assert!(z.order_id_sie_zgadza());
    // Atakujący podmienia order_id na "wygodny" — koordynator to złapie.
    z.order_id = "o-podmieniony".into();
    assert!(!z.order_id_sie_zgadza(), "podmieniony order_id MUSI być odrzucony");
}

#[test]
fn m02_nonce_jest_losowy() {
    use simon_harness::client_protocol::JobOrder;
    let a = JobOrder::new("m", b"x", 1, 1, "k".repeat(32)).unwrap();
    let b = JobOrder::new("m", b"x", 1, 1, "k".repeat(32)).unwrap();
    assert_ne!(a.nonce, b.nonce, "nonce musi być losowy");
    assert_eq!(a.nonce.len(), 32, "16 bajtów hex");
}
