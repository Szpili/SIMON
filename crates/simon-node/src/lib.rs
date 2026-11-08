//! SIMON node - node side.
//!
//! Node registry with public keys and job result handling with receipt verification.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use simon_core::receipt::Receipt;
use simon_core::{PublicKey, SimonError, PROTOCOL_VERSION};

/// Node state in coordinator registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeState {
    Registered,
    Healthy,
    Busy,
    Degraded,
    Evicted,
}

/// Compute capability declaration. `declared` = declared by node, `attested` = measured by coordinator.
///
/// RULES #2: routing uses exclusively `attested`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capability {
    pub declared: serde_json::Value,
    pub attested: Option<serde_json::Value>,
}

impl Capability {
    pub fn usable(&self) -> bool {
        self.attested.is_some()
    }
}

/// Domain separator for node registration proof of possession.
pub const NODE_REGISTER_DOMAIN: &str = "SIMON/NODE-REGISTER/v1";
pub const DOMENA_NODE_REGISTER: &str = NODE_REGISTER_DOMAIN;
/// Domain separator for operator voucher admitting a node into the network.
pub const NODE_VOUCHER_DOMAIN: &str = "SIMON/NODE-VOUCH/v1";
pub const DOMENA_NODE_VOUCHER: &str = NODE_VOUCHER_DOMAIN;

/// Node registration. `pubkey` declared once - node identity = this key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterMsg {
    pub node_id: String,
    pub protocol: String,
    pub capability: Capability,
    pub region: String,
    /// Variant C: public key included in registration.
    pub pubkey: PublicKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<simon_core::Signature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voucher_signature: Option<simon_core::Signature>,
}

impl RegisterMsg {
    pub fn new(node_id: impl Into<String>, pubkey: PublicKey, capability: Capability) -> Self {
        Self {
            node_id: node_id.into(),
            protocol: PROTOCOL_VERSION.to_string(),
            capability,
            region: "unknown".to_string(),
            pubkey,
            signature: None,
            voucher_signature: None,
        }
    }

    /// Computes deterministic digest of registration message (domain DOMENA_NODE_REGISTER).
    pub fn digest(&self) -> Result<String, SimonError> {
        simon_core::content_digest(&serde_json::json!({
            "domena": DOMENA_NODE_REGISTER,
            "node_id": self.node_id,
            "protocol": self.protocol,
            "declared": self.capability.declared,
            "region": self.region,
            "pubkey": self.pubkey,
        }))
    }

    /// Computes deterministic digest of operator voucher (domain DOMENA_NODE_VOUCHER).
    pub fn voucher_digest(&self) -> Result<String, SimonError> {
        simon_core::content_digest(&serde_json::json!({
            "domena": DOMENA_NODE_VOUCHER,
            "node_id": self.node_id,
            "protocol": self.protocol,
            "pubkey": self.pubkey,
        }))
    }

    /// Signs registration with node private key (proof of possession).
    pub fn sign(mut self, node_key: &simon_core::Keypair) -> Result<Self, SimonError> {
        self.pubkey = node_key.public();
        let digest = self.digest()?;
        self.signature = Some(node_key.sign_digest(&digest));
        Ok(self)
    }

    /// Attaches operator voucher signature.
    pub fn with_voucher(mut self, operator_key: &simon_core::Keypair) -> Result<Self, SimonError> {
        let digest = self.voucher_digest()?;
        self.voucher_signature = Some(operator_key.sign_digest(&digest));
        Ok(self)
    }

    /// Verifies node signature under its declared public key.
    pub fn verify_self(&self) -> Result<(), SimonError> {
        let sig = self.signature.as_ref().ok_or(SimonError::BadSignature)?;
        let digest = self.digest()?;
        self.pubkey.verify_digest(&digest, sig)
    }

    /// Verifies operator voucher signature under given operator public key.
    pub fn verify_voucher(&self, operator_pubkey: &PublicKey) -> Result<(), SimonError> {
        let sig = self
            .voucher_signature
            .as_ref()
            .ok_or(SimonError::BadSignature)?;
        let digest = self.voucher_digest()?;
        operator_pubkey.verify_digest(&digest, sig)
    }
}

/// Job result sent by node.
///
/// Result carries full `Receipt` for client verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: String,
    pub node_id: String,
    pub ok: bool,
    pub output: Option<serde_json::Value>,
    pub receipt: Option<Receipt>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<simon_core::Signature>,
}

/// Alias for signed job result on Gossipsub (`SIMON/JOB-RESULT/v1`).
pub type SignedJobResult = JobResult;

/// Domain separator for job result signatures.
pub const JOB_RESULT_DOMAIN: &str = "SIMON/JOB-RESULT/v1";
pub const DOMENA_JOB_RESULT: &str = JOB_RESULT_DOMAIN;

impl JobResult {
    /// Success with signed receipt.
    pub fn success(
        job_id: impl Into<String>,
        node_id: impl Into<String>,
        output: serde_json::Value,
        receipt: Receipt,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            node_id: node_id.into(),
            ok: true,
            output: Some(output),
            receipt: Some(receipt),
            error: None,
            signature: None,
        }
    }

    /// Failure - receipt not required.
    pub fn failure(
        job_id: impl Into<String>,
        node_id: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            node_id: node_id.into(),
            ok: false,
            output: None,
            receipt: None,
            error: Some(error.into()),
            signature: None,
        }
    }

    /// Computes deterministic digest of job result (domain DOMENA_JOB_RESULT).
    pub fn digest(&self) -> Result<String, SimonError> {
        let receipt_digest = self.receipt.as_ref().and_then(|r| r.digest().ok());
        simon_core::content_digest(&serde_json::json!({
            "domena": DOMENA_JOB_RESULT,
            "job_id": self.job_id,
            "node_id": self.node_id,
            "ok": self.ok,
            "error": self.error,
            "receipt_digest": receipt_digest,
        }))
    }

    /// Signs result with node key.
    pub fn sign(mut self, keypair: &simon_core::Keypair) -> Result<Self, SimonError> {
        let digest = self.digest()?;
        self.signature = Some(keypair.sign_digest(&digest));
        Ok(self)
    }

    /// Verifies result signature under registered node public key.
    pub fn verify_from_node(&self, registered_pubkey: &PublicKey) -> Result<(), SimonError> {
        let sig = self.signature.as_ref().ok_or(SimonError::BadSignature)?;
        let digest = self.digest()?;
        registered_pubkey.verify_digest(&digest, sig)
    }
}

/// Topic for node job results.
pub const TOPIC_JOB_RESULT: &str = "simon/v1/job-result";

/// Node and job registry. Deterministic, network-free.
#[derive(Default)]
pub struct Registry {
    pub nodes: HashMap<String, RegisterMsg>,
    pub states: HashMap<String, NodeState>,
    /// In-flight jobs: job_id -> node_id assigned to.
    pub assigned: HashMap<String, String>,
    pub timeout_s: Option<u64>,
    pub last_seen: HashMap<String, u64>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, msg: RegisterMsg) -> bool {
        if msg.protocol != PROTOCOL_VERSION {
            return false;
        }
        if self.nodes.contains_key(&msg.node_id) {
            return false;
        }
        self.nodes.insert(msg.node_id.clone(), msg.clone());
        self.states.insert(msg.node_id, NodeState::Registered);
        true
    }

    /// Registers node after checking protocol version, uniqueness,
    /// self-signature validity, and optional operator voucher signature.
    pub fn register_verified(
        &mut self,
        msg: RegisterMsg,
        operator_voucher_key: Option<&PublicKey>,
    ) -> Result<(), SimonError> {
        if msg.protocol != PROTOCOL_VERSION {
            return Err(SimonError::Crypto("mismatched protocol version".into()));
        }
        if self.nodes.contains_key(&msg.node_id) {
            return Err(SimonError::Crypto(
                "node with this id is already registered".into(),
            ));
        }
        msg.verify_self()?;
        if let Some(op) = operator_voucher_key {
            msg.verify_voucher(op)?;
        }
        self.nodes.insert(msg.node_id.clone(), msg.clone());
        self.states.insert(msg.node_id, NodeState::Registered);
        Ok(())
    }

    /// Candidates matching model and context limit.
    pub fn candidates_for_model(&self, model_hash: &str, required_ctx: u32) -> Vec<String> {
        let mut out: Vec<String> = self
            .nodes
            .values()
            .filter(|n| n.capability.usable())
            .filter(|n| {
                let d = &n.capability.declared;
                d.get("model_hash").and_then(|v| v.as_str()) == Some(model_hash)
            })
            .filter(|n| {
                required_ctx == 0 || {
                    let limit = n
                        .capability
                        .declared
                        .get("max_ctx")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    limit == 0 || required_ctx <= limit as u32
                }
            })
            .filter(|n| {
                !matches!(
                    self.states.get(&n.node_id),
                    Some(NodeState::Evicted) | Some(NodeState::Degraded)
                )
            })
            .map(|n| n.node_id.clone())
            .collect();
        out.sort();
        out
    }

    pub fn assign(&mut self, job_id: &str, node_id: &str) -> bool {
        // RULES #2: exclusively node with attested capability
        let Some(node) = self.nodes.get(node_id) else {
            return false;
        };
        if !node.capability.usable() {
            return false;
        }
        if matches!(
            self.states.get(node_id),
            Some(NodeState::Evicted) | Some(NodeState::Degraded)
        ) {
            return false;
        }
        self.assigned.insert(job_id.to_string(), node_id.to_string());
        true
    }

    /// Completes result. Requires valid receipt under registered node key.
    pub fn complete(&mut self, result: &JobResult) -> Result<(), SimonError> {
        let assigned_to = self
            .assigned
            .get(&result.job_id)
            .ok_or_else(|| SimonError::ReceiptMismatch("job is not assigned".into()))?;

        if assigned_to != &result.node_id {
            return Err(SimonError::ReceiptMismatch(
                "result from node not assigned to this job".into(),
            ));
        }

        if result.ok {
            let receipt = result
                .receipt
                .as_ref()
                .ok_or_else(|| SimonError::ReceiptMismatch("success without receipt".into()))?;

            let registered = self
                .nodes
                .get(&result.node_id)
                .ok_or(SimonError::UnregisteredKey)?;

            receipt.verify_for_node(&result.job_id, &result.node_id, &registered.pubkey)?;
        }

        self.assigned.remove(&result.job_id);
        Ok(())
    }

    /// Verifies if result originates from assigned node and is signed with its key.
    pub fn verify_result_from_assigned_node(&self, result: &JobResult) -> Result<(), SimonError> {
        if self.assigned.get(&result.job_id) != Some(&result.node_id) {
            return Err(SimonError::ReceiptMismatch(
                "result from node not assigned to this job".into(),
            ));
        }
        let registered = self
            .nodes
            .get(&result.node_id)
            .ok_or(SimonError::UnregisteredKey)?;
        result.verify_from_node(&registered.pubkey)?;
        Ok(())
    }

    /// Evicts node. Returns orphaned jobs for reassignment.
    pub fn evict(&mut self, node_id: &str) -> Vec<String> {
        if !self.nodes.contains_key(node_id) {
            return Vec::new();
        }
        self.states.insert(node_id.to_string(), NodeState::Evicted);
        let orphaned: Vec<String> = self
            .assigned
            .iter()
            .filter(|(_, n)| n.as_str() == node_id)
            .map(|(j, _)| j.clone())
            .collect();
        for job in &orphaned {
            self.assigned.remove(job);
        }
        orphaned
    }

    /// Marks node as degraded.
    pub fn mark_degraded(&mut self, node_id: &str) -> Result<(), SimonError> {
        if !self.nodes.contains_key(node_id) {
            return Err(SimonError::UnregisteredKey);
        }
        self.states.insert(node_id.to_string(), NodeState::Degraded);
        Ok(())
    }

    /// Restores Healthy state for registered node.
    pub fn heartbeat(&mut self, node_id: &str) -> Result<(), SimonError> {
        if !self.nodes.contains_key(node_id) {
            return Err(SimonError::UnregisteredKey);
        }
        self.states.insert(node_id.to_string(), NodeState::Healthy);
        Ok(())
    }

    /// Records node heartbeat with timestamp.
    pub fn record_heartbeat(&mut self, node_id: &str, now_s: u64) -> Result<(), SimonError> {
        if !self.nodes.contains_key(node_id) {
            return Err(SimonError::UnregisteredKey);
        }
        self.states.insert(node_id.to_string(), NodeState::Healthy);
        self.last_seen.insert(node_id.to_string(), now_s);
        Ok(())
    }

    /// Count of active assignments.
    pub fn active_assignments(&self) -> usize {
        self.assigned.len()
    }

    /// Removes job assignment by id without receipt verification.
    pub fn complete_job(&mut self, job_id: &str) -> Option<String> {
        self.assigned.remove(job_id)
    }

    /// Deterministic candidate selection via VRF seed.
    pub fn select_candidate_vrf(
        &self,
        model_hash: &str,
        required_ctx: u32,
        job_id: &str,
        now_s: u64,
    ) -> Option<(String, String)> {
        let candidates = self.candidates_for_model(model_hash, required_ctx);
        let candidates: Vec<String> = if let Some(timeout) = self.timeout_s {
            candidates
                .into_iter()
                .filter(|cand| {
                    if let Some(last) = self.last_seen.get(cand) {
                        now_s.saturating_sub(*last) <= timeout
                    } else {
                        true
                    }
                })
                .collect()
        } else {
            candidates
        };

        if candidates.is_empty() {
            return None;
        }

        let vrf_seed = simon_core::content_digest(&("SIMON/VRF/v1", &(model_hash, job_id))).ok()?;

        let mut best: Option<(String, String)> = None;
        for cand in candidates {
            let pick_digest =
                simon_core::content_digest(&("SIMON/VRF-PICK/v1", &(&vrf_seed, &cand))).ok()?;
            match &best {
                None => best = Some((cand, pick_digest)),
                Some((_, best_digest)) => {
                    if &pick_digest < best_digest {
                        best = Some((cand, pick_digest));
                    }
                }
            }
        }

        best.map(|(node_id, _)| (node_id, vrf_seed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simon_core::Keypair;

    fn test_node(id: &str, model: &str, max_ctx: u64) -> RegisterMsg {
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
        RegisterMsg::new(id, key.public(), cap)
    }

    #[test]
    fn test_mark_degraded_and_heartbeat_recovery() {
        let mut reg = Registry::new();
        let node = test_node("node-1", "model-a", 2048);
        assert!(reg.register(node));

        let candidates = reg.candidates_for_model("model-a", 1024);
        assert_eq!(candidates, vec!["node-1".to_string()]);

        assert!(reg.mark_degraded("node-1").is_ok());
        assert_eq!(reg.states.get("node-1"), Some(&NodeState::Degraded));
        assert!(reg.candidates_for_model("model-a", 1024).is_empty());
        assert!(!reg.assign("job-1", "node-1"));

        // Heartbeat restores node
        assert!(reg.heartbeat("node-1").is_ok());
        assert_eq!(reg.states.get("node-1"), Some(&NodeState::Healthy));
        assert_eq!(reg.candidates_for_model("model-a", 1024), vec!["node-1".to_string()]);
        assert!(reg.assign("job-1", "node-1"));
        assert_eq!(reg.active_assignments(), 1);
    }

    #[test]
    fn test_vrf_deterministic_selection() {
        let mut reg = Registry::new();
        for i in 1..=4 {
            let id = format!("node-{i}");
            reg.register(test_node(&id, "model-x", 4096));
        }

        let job_id = "job-42";
        let res1 = reg.select_candidate_vrf("model-x", 2048, job_id, 1000);
        let res2 = reg.select_candidate_vrf("model-x", 2048, job_id, 1000);
        let res3 = reg.select_candidate_vrf("model-x", 2048, job_id, 1000);

        assert!(res1.is_some());
        assert_eq!(res1, res2);
        assert_eq!(res2, res3);

        let (node, seed) = res1.unwrap();
        assert!(!node.is_empty());
        assert!(!seed.is_empty());
    }

    #[test]
    fn test_vrf_distribution() {
        let mut reg = Registry::new();
        for i in 1..=5 {
            let id = format!("node-{i}");
            reg.register(test_node(&id, "model-dist", 4096));
        }

        let mut selected_nodes = std::collections::HashSet::new();
        for j in 0..50 {
            let job_id = format!("job-{j}");
            if let Some((node, _)) = reg.select_candidate_vrf("model-dist", 1024, &job_id, 1000) {
                selected_nodes.insert(node);
            }
        }
        assert!(selected_nodes.len() > 1, "VRF must distribute across distinct nodes");
    }

    #[test]
    fn test_vrf_stale_node_timeout() {
        let mut reg = Registry::new();
        reg.timeout_s = Some(60);
        let n1 = test_node("node-fresh", "model-t", 2048);
        let n2 = test_node("node-stale", "model-t", 2048);
        reg.register(n1);
        reg.register(n2);

        reg.record_heartbeat("node-fresh", 1000).unwrap();
        reg.record_heartbeat("node-stale", 900).unwrap();

        let chosen = reg.select_candidate_vrf("model-t", 1024, "job-t", 1010);
        assert!(chosen.is_some());
        assert_eq!(chosen.unwrap().0, "node-fresh");
    }

    #[test]
    fn test_signed_job_result_success_and_failure() {
        let node_key = Keypair::generate();
        let wrong_key = Keypair::generate();

        // 1. Signed failure
        let fail = JobResult::failure("job-1", "node-1", "CUDA out of memory")
            .sign(&node_key)
            .expect("sign failure");
        assert!(fail.signature.is_some());
        assert!(fail.verify_from_node(&node_key.public()).is_ok());
        assert!(matches!(
            fail.verify_from_node(&wrong_key.public()),
            Err(SimonError::BadSignature)
        ));

        // 2. Unsigned failure rejected
        let unsigned_fail = JobResult::failure("job-1", "node-1", "CUDA out of memory");
        assert!(matches!(
            unsigned_fail.verify_from_node(&node_key.public()),
            Err(SimonError::BadSignature)
        ));

        // 3. Signed success
        let receipt = Receipt {
            job_id: "job-1".to_string(),
            node_id: "node-1".to_string(),
            model_hash: "sha256:model".to_string(),
            runtime: "vllm".to_string(),
            precision: simon_core::receipt::Precision::Bf16,
            activation_hash: "act".to_string(),
            output_digest: "sha256:out".to_string(),
            prompt_tokens: 10,
            completion_tokens: 20,
            started_at_us: 1000,
            finished_at_us: 2000,
            signer: node_key.public(),
            signature: None,
            bindings: None,
        }
        .sign(&node_key)
        .expect("sign receipt");

        let succ = JobResult::success("job-1", "node-1", serde_json::json!({"text": "ok"}), receipt)
            .sign(&node_key)
            .expect("sign success");
        assert!(succ.verify_from_node(&node_key.public()).is_ok());

        // 4. Tampering invalidates signature
        let mut tampered_job = fail.clone();
        tampered_job.job_id = "job-tampered".to_string();
        assert!(matches!(
            tampered_job.verify_from_node(&node_key.public()),
            Err(SimonError::BadSignature)
        ));

        let mut tampered_node = fail.clone();
        tampered_node.node_id = "node-tampered".to_string();
        assert!(matches!(
            tampered_node.verify_from_node(&node_key.public()),
            Err(SimonError::BadSignature)
        ));

        let mut tampered_err = fail.clone();
        tampered_err.error = Some("different error".to_string());
        assert!(matches!(
            tampered_err.verify_from_node(&node_key.public()),
            Err(SimonError::BadSignature)
        ));

        let mut tampered_ok = fail;
        tampered_ok.ok = true;
        assert!(matches!(
            tampered_ok.verify_from_node(&node_key.public()),
            Err(SimonError::BadSignature)
        ));
    }

    #[test]
    fn test_registry_verify_result_from_assigned_node() {
        let mut reg = Registry::new();
        let node_key = Keypair::generate();
        let cap = Capability {
            declared: serde_json::json!({"model_hash": "model-1"}),
            attested: Some(serde_json::json!({"model_hash": "model-1"})),
        };
        assert!(reg.register(RegisterMsg::new("node-1", node_key.public(), cap)));
        assert!(reg.assign("job-1", "node-1"));

        // Valid signed failure from assigned node
        let valid_res = JobResult::failure("job-1", "node-1", "OOM")
            .sign(&node_key)
            .unwrap();
        assert!(reg.verify_result_from_assigned_node(&valid_res).is_ok());

        // Result for unassigned job
        let unassigned_res = JobResult::failure("job-unassigned", "node-1", "OOM")
            .sign(&node_key)
            .unwrap();
        assert!(matches!(
            reg.verify_result_from_assigned_node(&unassigned_res),
            Err(SimonError::ReceiptMismatch(_))
        ));

        // Result from different node than assigned
        let wrong_node_key = Keypair::generate();
        let cap2 = Capability {
            declared: serde_json::json!({"model_hash": "model-1"}),
            attested: Some(serde_json::json!({"model_hash": "model-1"})),
        };
        assert!(reg.register(RegisterMsg::new("node-2", wrong_node_key.public(), cap2)));
        let wrong_node_res = JobResult::failure("job-1", "node-2", "OOM")
            .sign(&wrong_node_key)
            .unwrap();
        assert!(matches!(
            reg.verify_result_from_assigned_node(&wrong_node_res),
            Err(SimonError::ReceiptMismatch(_))
        ));

        // Unregistered node
        let unregistered_key = Keypair::generate();
        reg.assigned.insert("job-ghost".to_string(), "ghost-node".to_string());
        let ghost_res = JobResult::failure("job-ghost", "ghost-node", "OOM")
            .sign(&unregistered_key)
            .unwrap();
        assert!(matches!(
            reg.verify_result_from_assigned_node(&ghost_res),
            Err(SimonError::UnregisteredKey)
        ));
    }

    #[test]
    fn test_register_msg_proof_of_possession_and_spoofing() {
        let victim_key = Keypair::generate();
        let attacker_key = Keypair::generate();
        let cap = Capability {
            declared: serde_json::json!({"model_hash": "model-victim"}),
            attested: None,
        };

        // 1. Honest self-signature
        let honest_msg = RegisterMsg::new("victim-node", victim_key.public(), cap.clone())
            .sign(&victim_key)
            .expect("honest node can sign its registration");
        assert!(honest_msg.verify_self().is_ok());

        // 2. Attacker attempts to register victim key signed with attacker key
        let mut spoofed_msg = RegisterMsg::new("victim-node", victim_key.public(), cap)
            .sign(&attacker_key)
            .expect("attacker signs");
        // Attacker changes pubkey to victim but signature is from attacker_key
        spoofed_msg.pubkey = victim_key.public();
        assert!(matches!(
            spoofed_msg.verify_self(),
            Err(SimonError::BadSignature)
        ));

        // 3. Unsigned registration rejected by register_verified
        let mut reg = Registry::new();
        let unsigned = RegisterMsg::new("victim-node", victim_key.public(), Capability::default());
        assert!(reg.register_verified(unsigned, None).is_err());
    }

    #[test]
    fn test_sybil_flood_blocked_by_operator_voucher() {
        let operator_key = Keypair::generate();
        let mut reg = Registry::new();

        // 50 Sybil nodes with valid self-signatures (proof-of-possession),
        // but without operator voucher
        let mut blocked_sybils = 0;
        for i in 0..50 {
            let sybil_key = Keypair::generate();
            let cap = Capability {
                declared: serde_json::json!({"model_hash": "model-sybil"}),
                attested: None,
            };
            let sybil_msg = RegisterMsg::new(format!("sybil-{i}"), sybil_key.public(), cap)
                .sign(&sybil_key)
                .expect("sybil self-sign");

            let res = reg.register_verified(sybil_msg, Some(&operator_key.public()));
            if res.is_err() {
                blocked_sybils += 1;
            }
        }
        assert_eq!(blocked_sybils, 50, "All 50 Sybil nodes without voucher must be blocked!");
        assert_eq!(reg.nodes.len(), 0);

        // 2 authorized plant nodes with operator voucher
        for name in &["plant-node-1", "plant-node-2"] {
            let plant_key = Keypair::generate();
            let cap = Capability {
                declared: serde_json::json!({"model_hash": "model-plant"}),
                attested: None,
            };
            let plant_msg = RegisterMsg::new(*name, plant_key.public(), cap)
                .sign(&plant_key)
                .expect("plant self-sign")
                .with_voucher(&operator_key)
                .expect("plant operator voucher");

            assert!(reg.register_verified(plant_msg, Some(&operator_key.public())).is_ok());
        }

        assert_eq!(reg.nodes.len(), 2, "Only 2 authorized nodes with voucher should enter registry");
        assert!(reg.nodes.contains_key("plant-node-1"));
        assert!(reg.nodes.contains_key("plant-node-2"));
    }
}
