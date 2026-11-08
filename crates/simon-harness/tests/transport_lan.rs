//! Integration test: two swarms in a single process communicating over TCP.
//!
//! Verifies:
//!   1. peer discovery and connection (gossipsub handshake),
//!   2. message delivery across topic subscriptions,
//!   3. round-trip serialization preservation of order digest.

use std::time::Duration;

use libp2p::{Multiaddr, PeerId};
use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{JobOrder, TOPIC_JOB_ORDER};
use simon_harness::transport::{spawn_swarm, SimonEvent};

/// Collects swarm events within timeout limit.
async fn zbieraj(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<SimonEvent>,
    limit: Duration,
) -> Vec<SimonEvent> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let pozostalo = deadline.saturating_duration_since(tokio::time::Instant::now());
        if pozostalo.is_zero() {
            break;
        }
        match tokio::time::timeout(pozostalo, rx.recv()).await {
            Ok(Some(ev)) => out.push(ev),
            Ok(None) => break,
            Err(_) => break,
        }
    }
    out
}

/// Waits for a peer connection event.
async fn czekaj_na_peera(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<SimonEvent>,
    limit: Duration,
) -> Option<PeerId> {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let pozostalo = deadline.saturating_duration_since(tokio::time::Instant::now());
        if pozostalo.is_zero() {
            return None;
        }
        match tokio::time::timeout(pozostalo, rx.recv()).await {
            Ok(Some(SimonEvent::PeerConnected(p))) => return Some(p),
            Ok(Some(_)) => continue,
            _ => return None,
        }
    }
}

#[tokio::test]
async fn dwa_swarmy_lacza_sie_i_przekazuja_zlecenie() {
    let (node_a, mut ev_a) = spawn_swarm(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .await
        .expect("swarm A");
    let (node_b, mut ev_b) = spawn_swarm(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .await
        .expect("swarm B");

    node_a.subscribe(TOPIC_JOB_ORDER).expect("subscribe A");
    node_b.subscribe(TOPIC_JOB_ORDER).expect("subscribe B");

    // Discover ephemeral listen address of A.
    let addr_a = czekaj_na_adres(&mut ev_a, Duration::from_secs(10)).await;
    assert!(
        addr_a.is_some(),
        "swarm A did not report listen address"
    );
    let addr_a = addr_a.unwrap();

    // B dials A with peer ID.
    let z_p2p: Multiaddr = format!("{addr_a}/p2p/{}", node_a.peer_id)
        .parse()
        .expect("address with p2p");
    node_b.dial(z_p2p).expect("dial");

    let polaczenie = czekaj_na_peera(&mut ev_b, Duration::from_secs(20)).await;
    assert!(
        polaczenie.is_some(),
        "swarms failed to establish connection"
    );

    // Allow time for gossipsub mesh formation.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let key = Keypair::generate();
    let order = JobOrder::new(
        "sha256:model",
        b"test lan",
        100,
        300,
        key.public().to_hex(),
    )
    .expect("order");
    let bajty = serde_json::to_vec(&order).expect("serialization");
    let odcisk_przed = order.digest().expect("digest before");

    node_a.publish(TOPIC_JOB_ORDER, bajty).expect("publish");

    let eventy = zbieraj(&mut ev_b, Duration::from_secs(20)).await;
    let odebrane = eventy.iter().find_map(|e| match e {
        SimonEvent::Message { topic, data, .. } if topic == TOPIC_JOB_ORDER => Some(data.clone()),
        _ => None,
    });

    assert!(
        odebrane.is_some(),
        "order message did not reach second swarm (events: {})",
        eventy.len()
    );

    let odebrane_bajty = odebrane.unwrap();
    let odtworzone: JobOrder = serde_json::from_slice(&odebrane_bajty).expect("deserialization");
    assert_eq!(
        odtworzone.digest().unwrap(),
        odcisk_przed,
        "order digest changed over network roundtrip"
    );

    node_a.shutdown();
    node_b.shutdown();
}

async fn czekaj_na_adres(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<SimonEvent>,
    limit: Duration,
) -> Option<Multiaddr> {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let pozostalo = deadline.saturating_duration_since(tokio::time::Instant::now());
        if pozostalo.is_zero() {
            return None;
        }
        match tokio::time::timeout(pozostalo, rx.recv()).await {
            Ok(Some(SimonEvent::Listening(a))) => {
                if a.to_string().starts_with("/ip4/127.0.0.1") {
                    return Some(a);
                }
            }
            Ok(Some(_)) => continue,
            _ => return None,
        }
    }
}

#[tokio::test]
async fn dwa_swarmy_maja_rozne_peer_id() {
    let (a, _ea) = spawn_swarm(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .await
        .expect("A");
    let (b, _eb) = spawn_swarm(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .await
        .expect("B");
    assert_ne!(a.peer_id, b.peer_id, "each node must have distinct peer identity");
    a.shutdown();
    b.shutdown();
}

#[tokio::test]
async fn publikacja_bez_subskrybenta_nie_panikuje() {
    let (a, _ea) = spawn_swarm(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .await
        .expect("A");
    let wynik = a.publish(TOPIC_JOB_ORDER, b"test".to_vec());
    assert!(wynik.is_ok(), "publish to empty mesh accepted into queue");
    a.shutdown();
}
