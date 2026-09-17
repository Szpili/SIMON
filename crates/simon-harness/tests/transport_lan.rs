//! Test integracyjny: DWA swarmy w jednym procesie, realne połączenie TCP.
//!
//! To jest etap 1 z planu: sprawdza, że transport realnie działa, ZANIM
//! cokolwiek zainstalujemy na Franku i Macu. Bez tego testu instalacja byłaby
//! zgadywaniem.
//!
//! Sprawdza trzy rzeczy:
//!   1. czy dwa nody się znajdują i łączą (gossipsub handshake),
//!   2. czy wiadomość opublikowana przez jednego dochodzi do drugiego,
//!   3. czy round-trip bajtów NIE psuje odcisku zlecenia (podpis musi przetrwać).

use std::time::Duration;

use futures::StreamExt;
use libp2p::{Multiaddr, PeerId};
use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{JobOrder, TOPIC_JOB_ORDER};
use simon_harness::transport::{spawn_swarm, SimonEvent};

/// Zbiera eventy z swarmu przez `limit` czasu, zwraca te, które pasują.
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

/// Czeka na połączenie peera (gossipsub wymaga established connection + handshake).
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

    // Adres nasluchu A — system przydzielil port, wiec musimy go ODCZYTAC,
    // a nie zgadywac (dialowanie na port 0 nie ma sensu).
    let addr_a = czekaj_na_adres(&mut ev_a, Duration::from_secs(10)).await;
    assert!(
        addr_a.is_some(),
        "swarm A nie zglosil adresu nasluchu — nie ma jak go dialowac"
    );
    let addr_a = addr_a.unwrap();

    // B dialuje A. PeerId A dokladamy jako /p2p/, zeby polaczenie bylo
    // przypisane do wlasciwej tozsamosci (inaczej gossipsub nie zrobi handshake).
    let z_p2p: Multiaddr = format!("{addr_a}/p2p/{}", node_a.peer_id)
        .parse()
        .expect("adres z p2p");
    node_b.dial(z_p2p).expect("dial");

    // Gossipsub wymaga established connection ORAZ handshake protokolu.
    let polaczenie = czekaj_na_peera(&mut ev_b, Duration::from_secs(20)).await;
    assert!(
        polaczenie.is_some(),
        "swarmy nie nawiazaly polaczenia — gossipsub nie zadziala bez established peer"
    );

    // Gossipsub potrzebuje chwili na mesh formation po polaczeniu.
    // Bez tej zwloki publish moze trafic w pusty mesh (zmierzone: 0 odbiorcow).
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Zlecenie do rozgloszenia.
    let key = Keypair::generate();
    // M0.2: order_id liczony z treści + nonce.
    let order = JobOrder::new(
        "sha256:model",
        b"test lan",
        100,
        300,
        key.public().to_hex(),
    )
    .expect("zlecenie");
    let bajty = serde_json::to_vec(&order).expect("serializacja");
    let odcisk_przed = order.digest().expect("odcisk przed");

    node_a.publish(TOPIC_JOB_ORDER, bajty).expect("publish");

    let eventy = zbieraj(&mut ev_b, Duration::from_secs(20)).await;
    let odebrane = eventy.iter().find_map(|e| match e {
        SimonEvent::Message { topic, data, .. } if topic == TOPIC_JOB_ORDER => Some(data.clone()),
        _ => None,
    });

    assert!(
        odebrane.is_some(),
        "zlecenie nie dotarlo do drugiego swarmu (eventy: {})",
        eventy.len()
    );

    // KLUCZOWE: round-trip przez siec NIE MOZE zmienic odcisku.
    let odebrane_bajty = odebrane.unwrap();
    let odtworzone: JobOrder = serde_json::from_slice(&odebrane_bajty).expect("deserializacja");
    assert_eq!(
        odtworzone.digest().unwrap(),
        odcisk_przed,
        "odcisk zlecenia zmienil sie przez siec — podpis by nie przeszedl"
    );

    node_a.shutdown();
    node_b.shutdown();
}

/// Czeka na adres nasluchu lokalnego (pomija adresy IPv6/wildcard).
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
    assert_ne!(a.peer_id, b.peer_id, "każdy node ma własną tożsamość");
    a.shutdown();
    b.shutdown();
}

#[tokio::test]
async fn publikacja_bez_subskrybenta_nie_panikuje() {
    // Gossipsub bez peerów nie ma gdzie wysłać — to NIE jest błąd.
    // Kontrakt: publikacja na pusty swarm kończy się cicho (Err w logu), nie paniką.
    let (a, _ea) = spawn_swarm(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .await
        .expect("A");
    let wynik = a.publish(TOPIC_JOB_ORDER, b"test".to_vec());
    assert!(wynik.is_ok(), "kolejka przyjęła publikację");
    a.shutdown();
}
