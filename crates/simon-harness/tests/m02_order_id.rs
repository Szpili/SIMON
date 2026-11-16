// Test M0.2 - verifies nonce collision prevention and order_id verification.

#[test]
fn m02_stary_licznik_dawal_kolizje() {
    // Legacy mechanism: order_id = "o-{session}-{counter}". Identical requests
    // in different sessions with counter=1 collided.
    let stary = |sesja: &str, licznik: u64| format!("o-{sesja}-{licznik}");
    let a = stary("s1", 1);
    let b = stary("s1", 1);
    assert_eq!(a, b, "legacy mechanism produced duplicate order_id for two orders");

    // Current mechanism: order_id = digest(order with empty order_id + nonce).
    // Identical orders produce distinct order_id values via random nonce.
    use simon_harness::client_protocol::JobOrder;
    let z1 = JobOrder::new("sha256:model", b"same prompt", 100, 300, "aa".repeat(32)).unwrap();
    let z2 = JobOrder::new("sha256:model", b"same prompt", 100, 300, "aa".repeat(32)).unwrap();
    assert_ne!(z1.order_id, z2.order_id, "identical orders must yield distinct order_id via nonce");

    // Coordinator recomputes order_id from the order itself (recipient verification).
    assert!(z1.order_id_sie_zgadza(), "order_id = digest(order)");
    assert!(z2.order_id_sie_zgadza());
}

#[test]
fn m02_podmieniony_order_id_nie_przechodzi() {
    use simon_harness::client_protocol::JobOrder;
    let mut z = JobOrder::new("sha256:model", b"prompt", 100, 300, "aa".repeat(32)).unwrap();
    assert!(z.order_id_sie_zgadza());
    // Attacker modifies order_id - coordinator detects mismatch.
    z.order_id = "o-tampered".into();
    assert!(!z.order_id_sie_zgadza(), "tampered order_id must be rejected");
}

#[test]
fn m02_nonce_jest_losowy() {
    use simon_harness::client_protocol::JobOrder;
    let a = JobOrder::new("m", b"x", 1, 1, "k".repeat(32)).unwrap();
    let b = JobOrder::new("m", b"x", 1, 1, "k".repeat(32)).unwrap();
    assert_ne!(a.nonce, b.nonce, "nonce must be random");
    assert_eq!(a.nonce.len(), 32, "16 hex bytes");
}
