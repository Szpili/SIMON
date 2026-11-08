//! Role `coord` — coordinator: accepts orders, verifies authorization (D110),
//! broadcasts `OrderAccepted` (M0.1), and tracks spent authorizations (M0.4b).

use crate::common::{build_swarm, print_listen_addrs, SwarmBehaviourEvent};
use crate::Opcje;
use futures::StreamExt;
use libp2p::gossipsub::{Event as GsEvent, IdentTopic};
use libp2p::swarm::SwarmEvent;
use simon_core::crypto::Keypair;
use simon_harness::client_protocol::{
    JobOrder, OrderAccepted, SpentAuthorizationRegistry, TOPIC_JOB_ORDER, TOPIC_NODE_REGISTER,
    TOPIC_ORDER_ACCEPTED,
};
use simon_node::{JobResult, RegisterMsg, Registry, TOPIC_JOB_RESULT};

pub async fn run(opcje: &Opcje) -> Result<(), String> {
    let listen = opcje
        .listen
        .clone()
        .map(|l| vec![l])
        .unwrap_or_else(|| vec!["/ip4/0.0.0.0/tcp/9003".to_string()]);

    let mut swarm = build_swarm(&listen, &opcje.bootstrap, None)?;
    print_listen_addrs(&swarm, "coord");

    let key = Keypair::generate();
    println!("[coord] coordinator_pubkey={}", key.public().to_hex());

    let topic_orders = IdentTopic::new(TOPIC_JOB_ORDER);
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic_orders)
        .map_err(|e| format!("subscribe {TOPIC_JOB_ORDER}: {e}"))?;
    let topic_registration = IdentTopic::new(TOPIC_NODE_REGISTER);
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic_registration)
        .map_err(|e| format!("subscribe {TOPIC_NODE_REGISTER}: {e}"))?;
    let topic_results = IdentTopic::new(TOPIC_JOB_RESULT);
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic_results)
        .map_err(|e| format!("subscribe {TOPIC_JOB_RESULT}: {e}"))?;
    println!(
        "[coord] subscribed {TOPIC_JOB_ORDER} + {TOPIC_NODE_REGISTER} + {TOPIC_JOB_RESULT}; broadcasting on {TOPIC_ORDER_ACCEPTED}"
    );

    let mut spent_auths = SpentAuthorizationRegistry::default();
    let mut registry = Registry::new();

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::Behaviour(SwarmBehaviourEvent::Gossipsub(GsEvent::Message {
                message, ..
            })) => {
                if message.topic == topic_registration.hash() {
                    match serde_json::from_slice::<RegisterMsg>(&message.data) {
                        Ok(msg) => {
                            let _ = handle_registration(&mut registry, msg, None);
                        }
                        Err(e) => eprintln!("[coord] invalid registration: {e}"),
                    }
                    continue;
                }
                if message.topic == topic_results.hash() {
                    match serde_json::from_slice::<JobResult>(&message.data) {
                        Ok(res) => {
                            handle_job_result(&mut registry, res);
                        }
                        Err(e) => {
                            eprintln!(
                                "[coord] cannot parse result ({} B): {e}",
                                message.data.len()
                            );
                        }
                    }
                    continue;
                }
                if message.topic != topic_orders.hash() {
                    continue;
                }
                let Ok(order) = serde_json::from_slice::<JobOrder>(&message.data) else {
                    eprintln!("[coord] cannot parse order ({} B)", message.data.len());
                    continue;
                };
                handle_job_order(&mut swarm, &key, &mut spent_auths, &mut registry, order);
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("[coord] listening: {address}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("[coord] connected: {peer_id}");
            }
            _ => {}
        }
    }
}

pub fn handle_registration(
    registry: &mut Registry,
    msg: RegisterMsg,
    operator_voucher_key: Option<&simon_core::PublicKey>,
) -> bool {
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
    let node_id = msg.node_id.clone();

    match registry.register_verified(msg, operator_voucher_key) {
        Ok(()) => {
            println!("[coord] NODE registered: {node_id} model={model} max_ctx={ctx}");
            true
        }
        Err(e) => {
            eprintln!("[coord] REGISTRATION REJECTED ({e}) node={node_id}");
            false
        }
    }
}

pub fn handle_job_result(registry: &mut Registry, res: JobResult) {
    if registry.assigned.get(&res.job_id) != Some(&res.node_id) {
        println!(
            "[coord] RESULT IGNORED: job={} not assigned to node={}",
            res.job_id, res.node_id
        );
        return;
    }

    let Some(registered) = registry.nodes.get(&res.node_id) else {
        println!(
            "[coord] RESULT REJECTED (unknown node) node={} job={}",
            res.node_id, res.job_id
        );
        return;
    };

    if let Err(e) = res.verify_from_node(&registered.pubkey) {
        println!(
            "[coord] RESULT REJECTED (invalid node signature: {e}) node={} job={}",
            res.node_id, res.job_id
        );
        return;
    }

    let output_digest_empty = res
        .receipt
        .as_ref()
        .map(|r| r.output_digest.is_empty())
        .unwrap_or(true);

    if !res.ok || output_digest_empty {
        println!(
            "[coord] RESULT DEGRADED node={} job={} (ok={}, output_digest_empty={})",
            res.node_id, res.job_id, res.ok, output_digest_empty
        );
        let _ = registry.mark_degraded(&res.node_id);
        let _ = registry.complete_job(&res.job_id);
    } else {
        match registry.complete(&res) {
            Ok(()) => {
                println!(
                    "[coord] RESULT ACCEPTED job={} node={}",
                    res.job_id, res.node_id
                );
            }
            Err(e) => {
                println!(
                    "[coord] RESULT REJECTED ({e}) node={} job={} -> degradation",
                    res.node_id, res.job_id
                );
                let _ = registry.mark_degraded(&res.node_id);
                let _ = registry.complete_job(&res.job_id);
            }
        }
    }
}

pub fn process_job_order(
    key: &Keypair,
    spent_auths: &mut SpentAuthorizationRegistry,
    registry: &mut Registry,
    order: &JobOrder,
    now: u64,
) -> Result<(OrderAccepted, String, u32), String> {
    let digest = order
        .verify_authorization(now)
        .map_err(|e| format!("rejected authorization: {e}"))?;

    spent_auths
        .consume(&digest)
        .map_err(|e| format!("replayed authorization: {e}"))?;

    let job_id = format!("job-{}", &order.order_id[..16.min(order.order_id.len())]);

    let mut required_ctx = 512;
    let mut selected =
        registry.select_candidate_vrf(&order.model_hash, required_ctx, &job_id, now);
    if selected.is_none() {
        required_ctx = 0;
        selected = registry.select_candidate_vrf(&order.model_hash, required_ctx, &job_id, now);
    }

    let (selected_node, vrf_seed) = selected
        .ok_or_else(|| format!("no node serves model {}", order.model_hash))?;

    if !registry.assign(&job_id, &selected_node) {
        return Err(format!(
            "assignment error: node {selected_node} cannot accept job {job_id}"
        ));
    }

    let accepted = OrderAccepted {
        order_id: order.order_id.clone(),
        job_id,
        coordinator_pubkey: key.public().to_hex(),
        vrf_seed,
        vrf_block: 0,
        estimated_ms: 1000,
        signature: None,
    }
    .sign(key);

    Ok((accepted, selected_node, required_ctx))
}

fn handle_job_order(
    swarm: &mut libp2p::Swarm<crate::common::SwarmBehaviour>,
    key: &Keypair,
    spent_auths: &mut SpentAuthorizationRegistry,
    registry: &mut Registry,
    order: JobOrder,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    match process_job_order(key, spent_auths, registry, &order, now) {
        Ok((accepted, selected_node, ctx)) => {
            println!(
                "[coord] ACCEPTED order={} model={} fee={} selected={} ctx={}",
                order.order_id, order.model_hash, order.fee_think, selected_node, ctx
            );
            let Ok(bytes) = serde_json::to_vec(&accepted) else {
                eprintln!("[coord] cannot serialize accepted order");
                return;
            };
            let topic = IdentTopic::new(TOPIC_ORDER_ACCEPTED);
            if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic, bytes) {
                eprintln!("[coord] cannot broadcast acceptance: {e}");
            }
        }
        Err(e) => {
            println!("[coord] REJECTED order={} : {e}", order.order_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simon_core::receipt::{Precision, Receipt};
    use simon_harness::client_protocol::OrderAuthorization;
    use simon_node::{Capability, NodeState};

    fn register_test_node(reg: &mut Registry, id: &str, model: &str, max_ctx: u64) -> Keypair {
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
        let msg = RegisterMsg::new(id, key.public(), cap).sign(&key).unwrap();
        assert!(handle_registration(reg, msg, None));
        key
    }

    fn create_test_order(model: &str, client_key: &Keypair, now: u64) -> JobOrder {
        let mut order = JobOrder::new(
            model,
            b"prompt-test",
            100,
            300,
            client_key.public().to_hex(),
        )
        .unwrap();
        let aut = OrderAuthorization {
            v: 1,
            payload_digest: order.payload_digest.clone(),
            model_hash: order.model_hash.clone(),
            fee_think: order.fee_think,
            timeout_secs: order.timeout_secs,
            nonce: order.nonce.clone(),
            valid_until: now + 300,
            user_pubkey: client_key.public().to_hex(),
            signature: None,
        }
        .sign(client_key);
        order.authorization = Some(aut);
        order
    }

    #[test]
    fn test_coord_assignment_and_completion_lifecycle() {
        let coord_key = Keypair::generate();
        let client_key = Keypair::generate();
        let mut spent_auths = SpentAuthorizationRegistry::default();
        let mut reg = Registry::new();

        let node_key = register_test_node(&mut reg, "node-1", "sha256:model1", 2048);

        let now = 1000;
        let order = create_test_order("sha256:model1", &client_key, now);

        let res = process_job_order(&coord_key, &mut spent_auths, &mut reg, &order, now);
        assert!(res.is_ok());
        let (accepted, node, ctx) = res.unwrap();

        assert_eq!(node, "node-1");
        assert_eq!(ctx, 512);
        assert_eq!(accepted.order_id, order.order_id);
        assert_ne!(accepted.vrf_seed, "vrf-mvp");
        assert!(accepted.verify_signature());

        assert_eq!(reg.active_assignments(), 1);
        assert_eq!(
            reg.assigned.get(&accepted.job_id),
            Some(&"node-1".to_string())
        );

        let res_replay = process_job_order(&coord_key, &mut spent_auths, &mut reg, &order, now);
        assert!(res_replay.is_err());

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
            bindings: None,
        };
        receipt = receipt.sign(&node_key).unwrap();

        let job_res = JobResult::success(
            &accepted.job_id,
            "node-1",
            serde_json::json!({"text": "ok"}),
            receipt,
        )
        .sign(&node_key)
        .unwrap();
        handle_job_result(&mut reg, job_res);

        assert_eq!(reg.active_assignments(), 0);
        assert!(!reg.assigned.contains_key(&accepted.job_id));
        assert_ne!(reg.states.get("node-1"), Some(&NodeState::Degraded));
    }

    #[test]
    fn test_coord_empty_digest_or_failure_marks_degraded() {
        let coord_key = Keypair::generate();
        let client_key = Keypair::generate();
        let mut spent_auths = SpentAuthorizationRegistry::default();
        let mut reg = Registry::new();

        let node_key = register_test_node(&mut reg, "node-bad", "sha256:model2", 2048);

        let now = 1000;
        let order = create_test_order("sha256:model2", &client_key, now);

        let res = process_job_order(&coord_key, &mut spent_auths, &mut reg, &order, now);
        assert!(res.is_ok());
        let (accepted, _, _) = res.unwrap();

        let fail_res = JobResult::failure(&accepted.job_id, "node-bad", "CUDA OOM")
            .sign(&node_key)
            .unwrap();
        handle_job_result(&mut reg, fail_res);

        assert_eq!(reg.states.get("node-bad"), Some(&NodeState::Degraded));
    }

    #[test]
    fn foreign_result_does_not_degrade_node() {
        let mut reg = Registry::new();
        let _victim_key = register_test_node(&mut reg, "victim", "sha256:model2", 2048);
        handle_job_result(&mut reg, JobResult::failure("job-nonexistent", "victim", "fake"));
        assert_ne!(reg.states.get("victim"), Some(&NodeState::Degraded));

        let mut fake_success = JobResult::failure("job-other", "victim", "x");
        fake_success.ok = true;
        handle_job_result(&mut reg, fake_success);
        assert_ne!(reg.states.get("victim"), Some(&NodeState::Degraded));

        let other_key = register_test_node(&mut reg, "other", "sha256:model2", 2048);
        assert!(reg.assign("job-x", "other"));
        handle_job_result(&mut reg, JobResult::failure("job-x", "victim", "fake"));
        assert_ne!(reg.states.get("victim"), Some(&NodeState::Degraded));
        assert_eq!(reg.assigned.get("job-x").map(String::as_str), Some("other"));

        let legit_fail = JobResult::failure("job-x", "other", "OOM")
            .sign(&other_key)
            .unwrap();
        handle_job_result(&mut reg, legit_fail);
        assert_eq!(reg.states.get("other"), Some(&NodeState::Degraded));
        assert!(!reg.assigned.contains_key("job-x"));
    }

    #[test]
    fn unsigned_or_foreign_failure_rejected() {
        let mut reg = Registry::new();
        let other_key = register_test_node(&mut reg, "other", "sha256:model2", 2048);
        assert!(reg.assign("job-x", "other"));

        handle_job_result(&mut reg, JobResult::failure("job-x", "other", "forged unsigned OOM"));
        assert_ne!(reg.states.get("other"), Some(&NodeState::Degraded));
        assert_eq!(reg.assigned.get("job-x").map(String::as_str), Some("other"));

        let attacker_key = Keypair::generate();
        let forged_fail = JobResult::failure("job-x", "other", "forged wrong key OOM")
            .sign(&attacker_key)
            .unwrap();
        handle_job_result(&mut reg, forged_fail);
        assert_ne!(reg.states.get("other"), Some(&NodeState::Degraded));
        assert_eq!(reg.assigned.get("job-x").map(String::as_str), Some("other"));

        let legit_fail = JobResult::failure("job-x", "other", "OOM")
            .sign(&other_key)
            .unwrap();
        handle_job_result(&mut reg, legit_fail);
        assert_eq!(reg.states.get("other"), Some(&NodeState::Degraded));
        assert!(!reg.assigned.contains_key("job-x"));
    }

    #[test]
    fn sybil_without_voucher_rejected_in_voucher_mode() {
        let operator_key = Keypair::generate();
        let mut reg = Registry::new();

        let unsigned_key = Keypair::generate();
        let unsigned_msg = RegisterMsg::new(
            "unsigned-node",
            unsigned_key.public(),
            Capability::default(),
        );
        assert!(!handle_registration(&mut reg, unsigned_msg, None));
        assert!(reg.nodes.is_empty());

        for i in 0..50 {
            let sybil_key = Keypair::generate();
            let sybil_msg = RegisterMsg::new(
                format!("sybil-{i}"),
                sybil_key.public(),
                Capability::default(),
            )
            .sign(&sybil_key)
            .unwrap();
            assert!(!handle_registration(
                &mut reg,
                sybil_msg,
                Some(&operator_key.public())
            ));
        }
        assert!(reg.nodes.is_empty());

        let plant_key = Keypair::generate();
        let plant_msg = RegisterMsg::new(
            "plant-node-1",
            plant_key.public(),
            Capability::default(),
        )
        .sign(&plant_key)
        .unwrap()
        .with_voucher(&operator_key)
        .unwrap();
        assert!(handle_registration(
            &mut reg,
            plant_msg,
            Some(&operator_key.public())
        ));
        assert_eq!(reg.nodes.len(), 1);
    }

    #[test]
    fn test_coord_fallback_ctx_when_small_max_ctx() {
        let coord_key = Keypair::generate();
        let client_key = Keypair::generate();
        let mut spent_auths = SpentAuthorizationRegistry::default();
        let mut reg = Registry::new();

        register_test_node(&mut reg, "node-small", "sha256:model3", 256);

        let now = 1000;
        let order = create_test_order("sha256:model3", &client_key, now);

        let res = process_job_order(&coord_key, &mut spent_auths, &mut reg, &order, now);
        assert!(
            res.is_ok(),
            "coordinator should fall back to ctx=0 and assign node"
        );
        let (_, node, ctx) = res.unwrap();
        assert_eq!(node, "node-small");
        assert_eq!(ctx, 0);
    }
}
