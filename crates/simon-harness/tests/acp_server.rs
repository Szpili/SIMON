//! ACP Server tests (D105-D108, D110, D116).

use std::time::Duration;

use simon_core::crypto::Keypair;
use simon_harness::acp_server::{
    AcpServer, OrderPhase, SessionId, SignatureGate, UserOrderProposal,
};
use simon_harness::transport::{spawn_swarm, SimonEvent};

fn receipt_from_for(job_id: &str, node_id: &str, keypair: &Keypair, content: &str) -> simon_core::receipt::Receipt {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    let digest = hex::encode(h.finalize());
    let mut r = receipt_from(job_id, node_id, keypair);
    r.output_digest = digest;
    r
}

fn receipt_from(job_id: &str, node_id: &str, keypair: &Keypair) -> simon_core::receipt::Receipt {
    simon_core::receipt::Receipt {
        job_id: job_id.into(),
        node_id: node_id.into(),
        model_hash: "sha256:model".into(),
        runtime: "vllm-0.6.3".into(),
        precision: simon_core::receipt::Precision::Bf16,
        activation_hash: "toploc:258B".into(),
        output_digest: "sha256:result".into(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 100_000_000,
        finished_at_us: 100_400_000,
        signer: keypair.public(),
        signature: None,
        bindings: None,
    }
}

fn proposal(session: &str, fee: u64) -> UserOrderProposal {
    UserOrderProposal {
        session_id: SessionId(session.into()),
        content: "compute this".into(),
        model_hash: "sha256:model".into(),
        fee_think: fee,
        timeout_secs: 300,
        user_pubkey: Keypair::generate().public().to_hex(),
    }
}

fn signed_proposal(session: &str, fee: u64) -> (UserOrderProposal, String) {
    let k = Keypair::generate();
    let mut p = proposal(session, fee);
    p.user_pubkey = k.public().to_hex();
    let digest = p.digest().expect("digest");
    let sig = hex::encode(k.sign_digest(&digest).0);
    (p, sig)
}

fn submit_ok(srv: &mut AcpServer, session: &str, fee: u64) -> String {
    let (p, sig) = signed_proposal(session, fee);
    srv.submit(p, &sig).expect("signed proposal must succeed")
}

fn server() -> (AcpServer, tokio::sync::mpsc::UnboundedReceiver<SimonEvent>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (swarm, rx) = rt
        .block_on(spawn_swarm(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()]))
        .expect("swarm");
    std::mem::forget(rt);
    (AcpServer::new(swarm, Box::new(SignatureGate)), rx)
}

#[test]
fn d110_without_signature_proposal_rejected() {
    let (mut srv, _rx) = server();
    let res = srv.submit(proposal("s1", 100), "");
    assert!(res.is_err(), "D110: missing signature must reject proposal");

    let res2 = srv.submit(proposal("s1", 100), "user-sig-fake");
    assert!(res2.is_err(), "D110: arbitrary non-empty string is not a signature");

    let (p, sig) = signed_proposal("s1", 100);
    assert!(srv.submit(p, &sig).is_ok(), "valid Ed25519 signature succeeds");
}

#[test]
fn d110_without_user_pubkey_rejected() {
    let (mut srv, _rx) = server();
    let (mut p, sig) = signed_proposal("s2", 100);
    p.user_pubkey = String::new();
    assert!(srv.submit(p, &sig).is_err(), "D110: key required");
}

#[test]
fn d83_fee_zero_rejected() {
    let (mut srv, _rx) = server();
    let (p3, sig3) = signed_proposal("s3", 0);
    let res = srv.submit(p3, &sig3);
    assert!(res.is_err(), "D83: fee 0 -> no THINK emission");
}

#[test]
fn order_id_is_unique_in_session() {
    let (mut srv, _rx) = server();
    let a = submit_ok(&mut srv, "s4", 100);
    let b = submit_ok(&mut srv, "s4", 100);
    assert_ne!(a, b, "two orders in session = two distinct order_ids");
    assert_eq!(a.len(), 64, "order_id = sha256 hex");
}

#[test]
fn phase_broadcast_after_submit() {
    let (mut srv, _rx) = server();
    let id = submit_ok(&mut srv, "s5", 100);
    assert_eq!(srv.phase(&id), Some(&OrderPhase::Broadcast));
    assert_eq!(srv.faza(&id), Some(&OrderPhase::Broadcast));
}

#[test]
fn broadcast_publishes_orders() {
    let (mut srv, _rx) = server();
    submit_ok(&mut srv, "s6", 100);
    submit_ok(&mut srv, "s6", 200);
    let n = srv.broadcast().unwrap();
    assert_eq!(n, 2, "both authorized orders broadcast to gossipsub");
    assert_eq!(srv.broadcast().unwrap(), 0);
}

#[test]
fn progress_is_readable_for_client() {
    assert!(!OrderPhase::Broadcast.as_progress().is_empty());
    assert!(OrderPhase::Accepted {
        job_id: "j1".into()
    }
    .as_progress()
    .contains("j1"));
    assert!(OrderPhase::Error("timeout".into())
        .as_progress()
        .contains("timeout"));
}

#[test]
fn foreign_order_event_is_ignored() {
    let (mut srv, _rx) = server();
    let k = Keypair::generate();
    let foreign = simon_harness::client_protocol::OrderCompleted {
        order_id: "o-foreign-1".into(),
        job_id: "j-foreign".into(),
        node_id: "node-1".into(),
        receipt: receipt_from_for("j-foreign", "node-1", &k, "anything"),
        coordinator_pubkey: "coord".into(),
    };
    let bytes = serde_json::to_vec(&foreign).unwrap();
    let ev = SimonEvent::Message {
        topic: simon_harness::TOPIC_ORDER_COMPLETED.into(),
        data: bytes,
        from: libp2p::PeerId::random(),
    };
    assert!(srv.handle_event(&ev).is_none(), "foreign order ignored");
}

#[test]
fn receipt_verification_rejects_tampered_order() {
    let (mut srv, _rx) = server();
    let id = submit_ok(&mut srv, "s7", 100);

    let k = Keypair::generate();
    let job = simon_harness::transport::deterministic_job_id(&id);
    let receipt = receipt_from_for(&job, "node-x", &k, "result").sign(&k).expect("signature");

    let completed = simon_harness::client_protocol::OrderCompleted {
        order_id: id.clone(),
        job_id: job.clone(),
        node_id: "node-x".into(),
        receipt,
        coordinator_pubkey: "coord".into(),
    };

    assert!(srv.complete(&completed, &k.public(), "result").is_ok(), "valid result passes");

    let bad = srv.complete(&completed, &k.public(), "OTHER CONTENT");
    assert!(bad.is_err(), "mismatched content must be rejected");

    let mut tampered = completed.clone();
    tampered.job_id = "job-foreign".into();
    assert!(srv.complete(&tampered, &k.public(), "result").is_err(), "substituted job_id");

    let mut tampered2 = completed.clone();
    tampered2.receipt.model_hash = "sha256:OTHER".into();
    assert!(srv.complete(&tampered2, &k.public(), "result").is_err(), "substituted model");
}

#[test]
fn wait_for_completion_respects_timeout() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SimonEvent>();
    let res = rt.block_on(AcpServer::wait_for_completion(
        &mut rx,
        "o-nobody-1",
        Duration::from_millis(200),
    ));
    assert!(res.is_none(), "timeout returns None");
}
