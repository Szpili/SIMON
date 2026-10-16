//! Rola `coord` — koordynator: przyjmuje zlecenia, weryfikuje autoryzację (D110),
//! rozgłasza `OrderAccepted` (M0.1) i pilnuje zużytych autoryzacji (M0.4b).
//!
//! **M1.2:** minimalna rola — tyle, żeby zamknąć pętlę i przetestować bramkę
//! na drugiej maszynie. Pełny ranking/V Brouillera dochodzi w M2.

use crate::wspolne::{wypisz_adresy, zbuduj_swarm, ZachowanieEvent};
use crate::Opcje;
use futures::StreamExt;
use libp2p::gossipsub::{Event as GsEvent, IdentTopic};
use libp2p::swarm::SwarmEvent;
use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{
    JobOrder, OrderAccepted, RejestrZuzytychAutoryzacji, TOPIC_JOB_ORDER, TOPIC_NODE_REGISTER,
    TOPIC_ORDER_ACCEPTED,
};
use simon_node::{JobResult, RegisterMsg, Registry, TOPIC_JOB_RESULT};

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
    let topic_wyniki = IdentTopic::new(TOPIC_JOB_RESULT);
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic_wyniki)
        .map_err(|e| format!("subscribe {TOPIC_JOB_RESULT}: {e}"))?;
    println!(
        "[coord] subskrybuję {TOPIC_JOB_ORDER} + {TOPIC_NODE_REGISTER} + {TOPIC_JOB_RESULT}; rozgłaszam na {TOPIC_ORDER_ACCEPTED}"
    );

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
                if message.topic == topic_wyniki.hash() {
                    match serde_json::from_slice::<JobResult>(&message.data) {
                        Ok(res) => {
                            obsluz_wynik(&mut rejestr, res);
                        }
                        Err(e) => {
                            eprintln!(
                                "[coord] nie mogę sparsować wyniku ({} B): {e}",
                                message.data.len()
                            );
                        }
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
                obsluz_zlecenie(&mut swarm, &klucz, &mut zuzyte, &mut rejestr, order);
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

pub fn obsluz_wynik(rejestr: &mut Registry, res: JobResult) {
    // JobResult przychodzi z gossipsub od DOWOLNEGO peera, a node_id są publiczne (topic rejestracji).
    // Bez tej bramki każdy mógł jednym komunikatem zdegradować cudzy węzeł (porażka albo sfałszowany
    // „sukces” na nieprzydzielony job). Wynik liczy się tylko dla joba przydzielonego TEMU węzłowi.
    // ponytail: porażka nie jest podpisana, więc ktoś, kto zna świeżą parę (job_id, node_id), nadal może
    // zgłosić jej porażkę; pełna naprawa = JobResult podpisany kluczem node'a także przy porażce.
    if rejestr.assigned.get(&res.job_id) != Some(&res.node_id) {
        println!(
            "[coord] WYNIK ZIGNOROWANY: job={} nie jest przydzielony node={}",
            res.job_id, res.node_id
        );
        return;
    }
    let output_digest_pusty = res
        .receipt
        .as_ref()
        .map(|r| r.output_digest.is_empty())
        .unwrap_or(true);

    if !res.ok || output_digest_pusty {
        println!(
            "[coord] WYNIK ZDEGRADOWANY node={} job={} (ok={}, output_digest_empty={})",
            res.node_id, res.job_id, res.ok, output_digest_pusty
        );
        let _ = rejestr.mark_degraded(&res.node_id);
    } else {
        match rejestr.complete(&res) {
            Ok(()) => {
                println!(
                    "[coord] WYNIK ZATWIERDZONY job={} node={}",
                    res.job_id, res.node_id
                );
            }
            Err(e) => {
                println!(
                    "[coord] WYNIK ODRZUCONY ({e}) node={} job={} -> degradacja",
                    res.node_id, res.job_id
                );
                let _ = rejestr.mark_degraded(&res.node_id);
                let _ = rejestr.complete_job(&res.job_id);
            }
        }
    }
}

pub fn przetworz_zlecenie(
    klucz: &Keypair,
    zuzyte: &mut RejestrZuzytychAutoryzacji,
    rejestr: &mut Registry,
    order: &JobOrder,
    teraz: u64,
) -> Result<(OrderAccepted, String, u32), String> {
    // M0.4c: bramka koordynatora — wersja, zgodność pól, podpis, termin.
    let odcisk = order
        .zweryfikuj_autoryzacje(teraz)
        .map_err(|e| format!("odrzucona autoryzacja: {e}"))?;

    // M0.4b: ta sama autoryzacja nie może przejść dwa razy.
    zuzyte
        .zajmij(&odcisk)
        .map_err(|e| format!("powtórka autoryzacji: {e}"))?;

    let job_id = format!("job-{}", &order.order_id[..16.min(order.order_id.len())]);

    // Szacujemy wymagany kontekst: min 512 tokenów z fallbackiem do 0
    let mut wymagany_ctx = 512;
    let mut wybrany =
        rejestr.wybierz_kandydata_vrf(&order.model_hash, wymagany_ctx, &job_id, teraz);
    if wybrany.is_none() {
        wymagany_ctx = 0;
        wybrany = rejestr.wybierz_kandydata_vrf(&order.model_hash, wymagany_ctx, &job_id, teraz);
    }

    let (wybrany_node, vrf_seed) = wybrany
        .ok_or_else(|| format!("nikt nie serwuje modelu {}", order.model_hash))?;

    // Przypisanie w rejestrze przed rozgłoszeniem
    if !rejestr.assign(&job_id, &wybrany_node) {
        return Err(format!(
            "błąd przydziału: node {wybrany_node} nie mógł przyjąć joba {job_id}"
        ));
    }

    // M0.1: przyjęcie podpisane kluczem koordynatora z prawdziwym ziarnem VRF.
    let przyjecie = OrderAccepted {
        order_id: order.order_id.clone(),
        job_id,
        coordinator_pubkey: klucz.public().to_hex(),
        vrf_seed,
        vrf_block: 0,
        estimated_ms: 1000,
        signature: None,
    }
    .sign(klucz);

    Ok((przyjecie, wybrany_node, wymagany_ctx))
}

fn obsluz_zlecenie(
    swarm: &mut libp2p::Swarm<crate::wspolne::Zachowanie>,
    klucz: &Keypair,
    zuzyte: &mut RejestrZuzytychAutoryzacji,
    rejestr: &mut Registry,
    order: JobOrder,
) {
    let teraz = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    match przetworz_zlecenie(klucz, zuzyte, rejestr, &order, teraz) {
        Ok((przyjecie, wybrany_node, ctx)) => {
            println!(
                "[coord] PRZYJĘTE order={} model={} fee={} wybrany={} ctx={}",
                order.order_id, order.model_hash, order.fee_think, wybrany_node, ctx
            );
            let Ok(bajty) = serde_json::to_vec(&przyjecie) else {
                eprintln!("[coord] nie mogę serializować przyjęcia");
                return;
            };
            let topic = IdentTopic::new(TOPIC_ORDER_ACCEPTED);
            if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic, bajty) {
                eprintln!("[coord] nie mogę rozgłosić przyjęcia: {e}");
            }
        }
        Err(e) => {
            println!("[coord] ODRZUCONE order={} : {e}", order.order_id);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use simon_core::receipt::{Precision, Receipt};
    use simon_harness::client_protocol::Autoryzacja;
    use simon_node::{Capability, NodeState};

    fn zarejestruj_node(reg: &mut Registry, id: &str, model: &str, max_ctx: u64) -> Keypair {
        let key = Keypair::generate();
        let cap = Capability {
            declared: serde_json::json!({
                "model_hash": model,
                "max_ctx": max_ctx,
            }),
            attested: Some(serde_json::json!({
                "model_hash": model,
                "max_ctx": max_ctx,
            })),
        };
        assert!(reg.register(RegisterMsg::new(id, key.public(), cap)));
        key
    }

    fn stworz_zlecenie(model: &str, client_key: &Keypair, teraz: u64) -> JobOrder {
        let mut order = JobOrder::new(
            model,
            b"prompt-test",
            100,
            300,
            client_key.public().to_hex(),
        )
        .unwrap();
        let aut = Autoryzacja {
            v: 1,
            payload_digest: order.payload_digest.clone(),
            model_hash: order.model_hash.clone(),
            fee_think: order.fee_think,
            timeout_secs: order.timeout_secs,
            nonce: order.nonce.clone(),
            wazne_do: teraz + 300,
            user_pubkey: client_key.public().to_hex(),
            signature: None,
        }
        .sign(client_key);
        order.autoryzacja = Some(aut);
        order
    }

    #[test]
    fn test_coord_assignment_and_completion_lifecycle() {
        let coord_key = Keypair::generate();
        let client_key = Keypair::generate();
        let mut zuzyte = RejestrZuzytychAutoryzacji::default();
        let mut reg = Registry::new();

        let node_key = zarejestruj_node(&mut reg, "node-1", "sha256:model1", 2048);

        let teraz = 1000;
        let order = stworz_zlecenie("sha256:model1", &client_key, teraz);

        let res = przetworz_zlecenie(&coord_key, &mut zuzyte, &mut reg, &order, teraz);
        assert!(res.is_ok());
        let (accepted, node, ctx) = res.unwrap();

        assert_eq!(node, "node-1");
        assert_eq!(ctx, 512);
        assert_eq!(accepted.order_id, order.order_id);
        assert_ne!(accepted.vrf_seed, "vrf-mvp");
        assert!(accepted.podpis_zgadza_sie());

        // Sprawdzamy przypisanie w rejestrze
        assert_eq!(reg.active_assignments(), 1);
        assert_eq!(
            reg.assigned.get(&accepted.job_id),
            Some(&"node-1".to_string())
        );

        // Powtórka autoryzacji musi zostać odrzucona
        let res_powtorka = przetworz_zlecenie(&coord_key, &mut zuzyte, &mut reg, &order, teraz);
        assert!(res_powtorka.is_err());

        // Tworzymy poprawny JobResult z podpisem node'a
        let mut receipt = Receipt {
            job_id: accepted.job_id.clone(),
            node_id: "node-1".to_string(),
            model_hash: "sha256:model1".to_string(),
            runtime: "vllm".to_string(),
            precision: Precision::Bf16,
            activation_hash: "act".to_string(),
            output_digest: "sha256:output".to_string(),
            prompt_tokens: 10,
            completion_tokens: 20,
            started_at_us: 1000,
            finished_at_us: 2000,
            signer: node_key.public(),
            signature: None,
            wiazania: None,
        };
        receipt = receipt.sign(&node_key).unwrap();

        let job_res = JobResult::success(
            &accepted.job_id,
            "node-1",
            serde_json::json!({"text": "ok"}),
            receipt,
        );
        obsluz_wynik(&mut reg, job_res);

        // Job ukończony, rejestr odzyskał wolny stan
        assert_eq!(reg.active_assignments(), 0);
        assert!(!reg.assigned.contains_key(&accepted.job_id));
        assert_ne!(reg.states.get("node-1"), Some(&NodeState::Degraded));
    }

    #[test]
    fn test_coord_empty_digest_or_failure_marks_degraded() {
        let coord_key = Keypair::generate();
        let client_key = Keypair::generate();
        let mut zuzyte = RejestrZuzytychAutoryzacji::default();
        let mut reg = Registry::new();

        let _node_key = zarejestruj_node(&mut reg, "node-bad", "sha256:model2", 2048);

        let teraz = 1000;
        let order = stworz_zlecenie("sha256:model2", &client_key, teraz);

        let res = przetworz_zlecenie(&coord_key, &mut zuzyte, &mut reg, &order, teraz);
        assert!(res.is_ok());
        let (accepted, _, _) = res.unwrap();

        // Porażka wykonania
        let fail_res = JobResult::failure(&accepted.job_id, "node-bad", "CUDA OOM");
        obsluz_wynik(&mut reg, fail_res);

        assert_eq!(reg.states.get("node-bad"), Some(&NodeState::Degraded));
    }

    #[test]
    fn obcy_wynik_nie_degraduje_wezla() {
        let mut reg = Registry::new();
        zarejestruj_node(&mut reg, "ofiara", "sha256:model2", 2048);
        // porażka dla joba, którego nikt nie przydzielił
        obsluz_wynik(&mut reg, JobResult::failure("job-nieistniejacy", "ofiara", "fake"));
        assert_ne!(reg.states.get("ofiara"), Some(&NodeState::Degraded));
        // „sukces” bez receiptu (output_digest pusty) — też ignorowany
        let mut fałszywy = JobResult::failure("job-inny", "ofiara", "x");
        fałszywy.ok = true;
        obsluz_wynik(&mut reg, fałszywy);
        assert_ne!(reg.states.get("ofiara"), Some(&NodeState::Degraded));
        // job przydzielony INNEMU węzłowi nie degraduje ofiary
        zarejestruj_node(&mut reg, "inny", "sha256:model2", 2048);
        assert!(reg.assign("job-x", "inny"));
        obsluz_wynik(&mut reg, JobResult::failure("job-x", "ofiara", "fake"));
        assert_ne!(reg.states.get("ofiara"), Some(&NodeState::Degraded));
        assert_eq!(reg.assigned.get("job-x").map(String::as_str), Some("inny"));
        // próba kontrolna: porażka od węzła, któremu job PRZYDZIELONO, nadal degraduje
        obsluz_wynik(&mut reg, JobResult::failure("job-x", "inny", "OOM"));
        assert_eq!(reg.states.get("inny"), Some(&NodeState::Degraded));
    }

    #[test]
    fn test_coord_fallback_ctx_when_small_max_ctx() {
        let coord_key = Keypair::generate();
        let client_key = Keypair::generate();
        let mut zuzyte = RejestrZuzytychAutoryzacji::default();
        let mut reg = Registry::new();

        // Węzeł ma max_ctx = 256 (< 512 wymaganych domyślnie)
        zarejestruj_node(&mut reg, "node-small", "sha256:model3", 256);

        let teraz = 1000;
        let order = stworz_zlecenie("sha256:model3", &client_key, teraz);

        let res = przetworz_zlecenie(&coord_key, &mut zuzyte, &mut reg, &order, teraz);
        assert!(
            res.is_ok(),
            "Koordynator powinien spaść do ctx=0 i przydzielić węzeł"
        );
        let (_, node, ctx) = res.unwrap();
        assert_eq!(node, "node-small");
        assert_eq!(ctx, 0);
    }
}
