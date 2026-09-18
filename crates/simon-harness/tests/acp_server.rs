//! Testy serwera ACP (D105–D108, D110, D116).
//!
//! Sprawdzają to, co jest kontraktem wobec harnessów — nie tylko kompilację.

use std::time::Duration;

use simon_core::crypto::Keypair;
use simon_harness::acp_server::{
    AcpServer, BramkaPodpisu, FazaZlecenia, SessionId, ZlecenieUzytkownika,
};
use simon_harness::transport::{spawn_swarm, SimonEvent};


fn receipt_z_dla(job_id: &str, node_id: &str, klucz: &Keypair, tresc: &str) -> simon_core::receipt::Receipt {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(tresc.as_bytes());
    let odcisk = hex::encode(h.finalize());
    let mut r = receipt_z(job_id, node_id, klucz);
    r.output_digest = odcisk;
    r
}

fn receipt_z(job_id: &str, node_id: &str, klucz: &Keypair) -> simon_core::receipt::Receipt {
    simon_core::receipt::Receipt {
        job_id: job_id.into(),
        node_id: node_id.into(),
        model_hash: "sha256:model".into(),
        runtime: "vllm-0.6.3".into(),
        precision: simon_core::receipt::Precision::Bf16,
        activation_hash: "toploc:258B".into(),
        output_digest: "sha256:wynik".into(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 100_000_000,
        finished_at_us: 100_400_000,
        signer: klucz.public(),
        signature: None,
    }
}

fn zlecenie(session: &str, fee: u64) -> ZlecenieUzytkownika {
    ZlecenieUzytkownika {
        session_id: SessionId(session.into()),
        tresc: "policz to".into(),
        model_hash: "sha256:model".into(),
        fee_think: fee,
        timeout_secs: 300,
        user_pubkey: Keypair::generate().public().to_hex(),
    }
}

/// M0.3: bramka weryfikuje Ed25519, więc testy muszą podpisywać NAPRAWDĘ.
/// Zwraca (zlecenie, podpis).
fn zlecenie_podpisane(session: &str, fee: u64) -> (ZlecenieUzytkownika, String) {
    let k = Keypair::generate();
    let mut z = zlecenie(session, fee);
    z.user_pubkey = k.public().to_hex();
    let odcisk = z.digest().expect("odcisk");
    let podpis = hex::encode(k.sign_digest(&odcisk).0.to_vec());
    (z, podpis)
}

/// Skrót: przyjmij podpisane zlecenie.
fn przyjmij_ok(srv: &mut AcpServer, session: &str, fee: u64) -> String {
    let (z, p) = zlecenie_podpisane(session, fee);
    srv.przyjmij(z, &p).expect("podpisane zlecenie ma przejść")
}

fn serwer() -> (AcpServer, tokio::sync::mpsc::UnboundedReceiver<SimonEvent>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (swarm, rx) = rt
        .block_on(spawn_swarm(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()]))
        .expect("swarm");
    std::mem::forget(rt); // utrzymaj runtime przy życiu dla swarmu
    (AcpServer::new(swarm, Box::new(BramkaPodpisu)), rx)
}

#[test]
fn d110_bez_podpisu_zlecenie_nie_powstaje() {
    let (mut srv, _rx) = serwer();
    let wynik = srv.przyjmij(zlecenie("s1", 100), "");
    assert!(wynik.is_err(), "D110: brak podpisu MUSI blokować zlecenie");

    // M0.3: "byle niepusty string" NIE jest podpisem (kontrola niepustości to był teatr).
    let wynik2 = srv.przyjmij(zlecenie("s1", 100), "podpis-uzytkownika");
    assert!(wynik2.is_err(), "D110: niepusty string to NIE podpis");

    // Prawdziwy podpis Ed25519 przechodzi.
    let (z, p) = zlecenie_podpisane("s1", 100);
    assert!(srv.przyjmij(z, &p).is_ok(), "z prawdziwym podpisem zlecenie ma powstać");
}

#[test]
fn d110_bez_klucza_uzytkownika_blokada() {
    let (mut srv, _rx) = serwer();
    let (mut z, p) = zlecenie_podpisane("s2", 100);
    z.user_pubkey = String::new();
    assert!(srv.przyjmij(z, &p).is_err(), "D110: klucz wymagany");
}

#[test]
fn d83_oplata_zero_odrzucona() {
    let (mut srv, _rx) = serwer();
    let (z3, p3) = zlecenie_podpisane("s3", 0);
    let wynik = srv.przyjmij(z3, &p3);
    assert!(wynik.is_err(), "D83: opłata 0 → brak emisji THINK");
}

#[test]
fn order_id_jest_unikalny_w_sesji() {
    let (mut srv, _rx) = serwer();
    let a = przyjmij_ok(&mut srv, "s4", 100);
    let b = przyjmij_ok(&mut srv, "s4", 100);
    assert_ne!(a, b, "dwa zlecenia w sesji = dwa różne order_id");
    assert_eq!(a.len(), 64, "order_id = sha256 hex");
}

#[test]
fn faza_rozgloszone_po_przyjeciu() {
    let (mut srv, _rx) = serwer();
    let id = przyjmij_ok(&mut srv, "s5", 100);
    assert_eq!(srv.faza(&id), Some(&FazaZlecenia::Rozgloszone));
}

#[test]
fn rozgloszenie_publikuje_zlecenia() {
    let (mut srv, _rx) = serwer();
    przyjmij_ok(&mut srv, "s6", 100);
    przyjmij_ok(&mut srv, "s6", 200);
    let n = srv.rozglos().unwrap();
    assert_eq!(n, 2, "oba autoryzowane zlecenia idą na gossipsub");

    // Drugi raz nic nie zostało — nie ma podwójnej publikacji.
    assert_eq!(srv.rozglos().unwrap(), 0);
}

#[test]
fn postep_jest_czytelny_dla_klienta() {
    // AgentThoughtChunk — to widzi klient na żywo.
    assert!(!FazaZlecenia::Rozgloszone.jako_postep().is_empty());
    assert!(FazaZlecenia::Przyjete {
        job_id: "j1".into()
    }
    .jako_postep()
    .contains("j1"));
    assert!(FazaZlecenia::Blad("timeout".into())
        .jako_postep()
        .contains("timeout"));
}

#[test]
fn event_obcego_zlecenia_jest_ignorowany() {
    let (mut srv, _rx) = serwer();
    // Zdarzenie o order_id, którego nie znamy → brak reakcji (D107).
    let klucz = Keypair::generate();
    let obce = simon_harness::client_protocol::OrderCompleted {
        order_id: "o-obce-1".into(),
        job_id: "j-obce".into(),
        node_id: "node-1".into(),
        receipt: receipt_z_dla("j-obce", "node-1", &klucz, "cokolwiek"),
        coordinator_pubkey: "coord".into(),
    };
    let bajty = serde_json::to_vec(&obce).unwrap();
    let ev = SimonEvent::Message {
        topic: simon_harness::TOPIC_ORDER_COMPLETED.into(),
        data: bajty,
        from: libp2p::PeerId::random(),
    };
    assert!(srv.obsluz_event(&ev).is_none(), "obce zlecenie ignorowane");
}

#[test]
fn weryfikacja_receiptu_odrzuca_podmienione_zlecenie() {
    let (mut srv, _rx) = serwer();
    let id = przyjmij_ok(&mut srv, "s7", 100);

    let klucz = Keypair::generate();
    let job = simon_harness::transport::deterministic_job_id(&id);
    let receipt = receipt_z_dla(&job, "node-x", &klucz, "wynik").sign(&klucz).expect("podpis");

    let completed = simon_harness::client_protocol::OrderCompleted {
        order_id: id.clone(),
        job_id: job.clone(),
        node_id: "node-x".into(),
        receipt,
        coordinator_pubkey: "coord".into(),
    };

    // Najpierw: poprawny przypadek przechodzi (kontrola, że test ma sens).
    assert!(srv.zakoncz(&completed, &klucz.public(), "wynik").is_ok(), "poprawny wynik ma przejść");

    // Podstawienie 1: inna treść niż podpisana (output_digest).
    let zla = srv.zakoncz(&completed, &klucz.public(), "INNA TRESC");
    assert!(zla.is_err(), "P1: treść niepowiązana z receiptem MUSI być odrzucona");

    // Podstawienie 2: inny job_id.
    let mut podm = completed.clone();
    podm.job_id = "job-obcy".into();
    assert!(srv.zakoncz(&podm, &klucz.public(), "wynik").is_err(), "P1: podstawiony job_id");

    // Podstawienie 3: inny model w receipcie.
    let mut podm2 = completed.clone();
    podm2.receipt.model_hash = "sha256:INNY".into();
    assert!(srv.zakoncz(&podm2, &klucz.public(), "wynik").is_err(), "P1: podstawiony model");
}

#[test]
fn czekanie_na_zakonczenie_respektuje_timeout() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SimonEvent>();
    let wynik = rt.block_on(AcpServer::czekaj_na_zakonczenie(
        &mut rx,
        "o-nikt-1",
        Duration::from_millis(200),
    ));
    assert!(wynik.is_none(), "brak zdarzenia → timeout zwraca None");
}
