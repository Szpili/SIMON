//! Rola `coord` — koordynator: przyjmuje zlecenia, weryfikuje autoryzację (D110),
//! rozgłasza `OrderAccepted` (M0.1) i pilnuje zużytych autoryzacji (M0.4b).
//!
//! **M1.2:** minimalna rola — tyle, żeby zamknąć pętlę i przetestować bramkę
//! na drugiej maszynie. Pełny ranking/V Brouillera dochodzi w M2.

use crate::wspolne::{wypisz_adresy, zbuduj_swarm, ZachowanieEvent};
use crate::Opcje;
use futures::StreamExt;
use libp2p::gossipsub::{Event as GsEvent, IdentTopic, MessageAuthenticity};
use libp2p::swarm::SwarmEvent;
use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{
    JobOrder, OrderAccepted, RejestrZuzytychAutoryzacji, TOPIC_JOB_ORDER, TOPIC_NODE_REGISTER,
    TOPIC_ORDER_ACCEPTED,
};
use simon_node::{RegisterMsg, Registry};

pub async fn uruchom(opcje: &Opcje) -> Result<(), String> {
    let listen = opcje
        .listen
        .clone()
        .map(|l| vec![l])
        .unwrap_or_else(|| vec!["/ip4/0.0.0.0/tcp/9003".to_string()]);

    // Losowy peer_id per proces — patrz komentarz w node.rs.
    let mut swarm = zbuduj_swarm(&listen, &opcje.bootstrap, None)?;
    wypisz_adresy(&swarm, "coord");

    // Koordynator musi mieć tożsamość w rejestrze — inaczej jego przyjęcia są odrzucane.
    let klucz = Keypair::generate();
    println!("[coord] coordinator_pubkey={}", klucz.public().to_hex());

    let topic_zlecenia = IdentTopic::new(TOPIC_JOB_ORDER);
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic_zlecenia)
        .map_err(|e| format!("subscribe {TOPIC_JOB_ORDER}: {e}"))?;
    // C-gate: koordynator słucha też rejestracji node'ów, żeby wiedzieć,
    // KTO ma JAKI model i jaki limit kontekstu. Bez tego przyjmował zlecenie
    // na model, którego nikt nie serwuje — i cicho wisiało.
    let topic_rejestracji = IdentTopic::new(TOPIC_NODE_REGISTER);
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic_rejestracji)
        .map_err(|e| format!("subscribe {TOPIC_NODE_REGISTER}: {e}"))?;
    println!("[coord] subskrybuję {TOPIC_JOB_ORDER} + {TOPIC_NODE_REGISTER}; rozgłaszam na {TOPIC_ORDER_ACCEPTED}");

    let mut zuzyte = RejestrZuzytychAutoryzacji::default();
    let mut rejestr = Registry::new();

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::Behaviour(ZachowanieEvent::Gossipsub(GsEvent::Message {
                message, ..
            })) => {
                // C-gate: rejestracja node'a — zapisujemy, co realnie oferuje.
                if message.topic == topic_rejestracji.hash() {
                    match serde_json::from_slice::<RegisterMsg>(&message.data) {
                        Ok(msg) => {
                            let model = msg
                                .capability
                                .declared
                                .get("model_hash")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?")
                                .to_string();
                            let ctx = msg
                                .capability
                                .declared
                                .get("max_ctx")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            if rejestr.register(msg.clone()) {
                                println!(
                                    "[coord] NODE zarejestrowany: {} model={model} max_ctx={ctx}",
                                    msg.node_id
                                );
                            }
                        }
                        Err(e) => eprintln!("[coord] zła rejestracja: {e}"),
                    }
                    continue;
                }
                if message.topic != topic_zlecenia.hash() {
                    continue;
                }
                let Ok(order) = serde_json::from_slice::<JobOrder>(&message.data) else {
                    eprintln!("[coord] nie mogę sparsować zlecenia ({} B)", message.data.len());
                    continue;
                };
                obsluz_zlecenie(&mut swarm, &klucz, &mut zuzyte, &rejestr, order);
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("[coord] nasłuchuję: {address}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("[coord] połączony: {peer_id}");
            }
            _ => {}
        }
    }
}

fn obsluz_zlecenie(
    swarm: &mut libp2p::Swarm<crate::wspolne::Zachowanie>,
    klucz: &Keypair,
    zuzyte: &mut RejestrZuzytychAutoryzacji,
    rejestr: &Registry,
    order: JobOrder,
) {
    let teraz = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // M0.4c: bramka koordynatora — wersja, zgodność pól, podpis, termin.
    let odcisk = match order.zweryfikuj_autoryzacje(teraz) {
        Ok(o) => o,
        Err(e) => {
            println!("[coord] ODRZUCONE order={} : {e}", order.order_id);
            return;
        }
    };
    // M0.4b: ta sama autoryzacja nie może przejść dwa razy.
    if let Err(e) = zuzyte.zajmij(&odcisk) {
        println!("[coord] ODRZUCONE (powtórka) {}: {e}", order.order_id);
        return;
    }

    // C-gate: czy KTOKOLWIEK serwuje ten model? Bez tego przyjmowaliśmy
    // zlecenie, które nie miało na czym się policzyć — cicha odmowa.
    let kandydaci = rejestr.kandydaci_dla_modelu(&order.model_hash, 0);
    if kandydaci.is_empty() {
        println!(
            "[coord] ODRZUCONE order={} : nikt nie serwuje modelu {}",
            order.order_id, order.model_hash
        );
        return;
    }

    println!(
        "[coord] PRZYJĘTE order={} model={} fee={} kandydaci={}",
        order.order_id,
        order.model_hash,
        order.fee_think,
        kandydaci.len()
    );

    // M0.1: przyjęcie podpisane kluczem koordynatora.
    let job_id = format!("job-{}", &order.order_id[..16.min(order.order_id.len())]);
    let przyjecie = OrderAccepted {
        order_id: order.order_id.clone(),
        job_id,
        coordinator_pubkey: klucz.public().to_hex(),
        vrf_seed: "vrf-mvp".into(),
        vrf_block: 0,
        estimated_ms: 1000,
        signature: None,
    }
    .sign(klucz);

    let Ok(bajty) = serde_json::to_vec(&przyjecie) else {
        eprintln!("[coord] nie mogę serializować przyjęcia");
        return;
    };
    let topic = IdentTopic::new(TOPIC_ORDER_ACCEPTED);
    if let Err(e) = swarm
        .behaviour_mut()
        .gossipsub
        .publish(topic, bajty)
    {
        eprintln!("[coord] nie mogę rozgłosić przyjęcia: {e}");
    }
}

/// Trzyma typ w zasięgu (podpisany publikator wymaga klucza).
#[allow(dead_code)]
fn _podpisany() -> MessageAuthenticity {
    MessageAuthenticity::RandomAuthor
}
