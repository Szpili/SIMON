//! ACP Server for SIMON — interface layer for external harnesses.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use simon_core::SimonError;

use crate::client_protocol::{
    JobOrder, OrderAccepted, OrderCompleted, TOPIC_JOB_ORDER,
    TOPIC_ORDER_ACCEPTED, TOPIC_ORDER_COMPLETED,
};
use crate::transport::{SimonEvent, SwarmHandle};

/// ACP session identifier (conversation client <-> agent).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

/// Order phase — exposed to client as progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderPhase {
    /// Order broadcast to the swarm — awaiting coordinator.
    Broadcast,
    /// Coordinator accepted order.
    Accepted { job_id: String },
    /// Result arrived — receipt verification in progress.
    Verifying,
    /// Result verified — ready to display.
    Completed,
    /// Error with reason.
    Error(String),
}

pub type FazaZlecenia = OrderPhase;

impl OrderPhase {
    /// Human-readable progress text for `AgentThoughtChunk`.
    pub fn as_progress(&self) -> String {
        match self {
            Self::Broadcast => "Order broadcast to swarm...".into(),
            Self::Accepted { job_id } => format!("Coordinator accepted order ({job_id})."),
            Self::Verifying => "Result received — verifying receipt...".into(),
            Self::Completed => "Result verified.".into(),
            Self::Error(e) => format!("Error: {e}"),
        }
    }

    pub fn jako_postep(&self) -> String {
        self.as_progress()
    }
}

/// User order proposal (ACP `PromptRequest` equivalent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOrderProposal {
    pub session_id: SessionId,
    #[serde(rename = "content", alias = "tresc")]
    pub content: String,
    pub model_hash: String,
    pub fee_think: u64,
    pub timeout_secs: u32,
    pub user_pubkey: String,
}

pub type ZlecenieUzytkownika = UserOrderProposal;

impl UserOrderProposal {
    /// Order proposal content digest for user signature (M0.3, D110).
    pub fn digest(&self) -> Result<String, SimonError> {
        simon_core::content_digest(self)
    }
}

/// Order result ready to be returned to caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResult {
    pub order_id: String,
    pub job_id: String,
    #[serde(rename = "content", alias = "tresc")]
    pub content: String,
    pub receipt: simon_core::receipt::Receipt,
}

pub type WynikZlecenia = OrderResult;

/// Authorization gate (D110).
pub trait AuthorizationGate: Send + Sync {
    /// Returns `Ok(())` if proposal is validly signed by the user.
    fn authorize(&self, proposal: &UserOrderProposal, signature: &str) -> Result<(), SimonError>;

    fn autoryzuj(&self, proposal: &UserOrderProposal, signature: &str) -> Result<(), SimonError> {
        self.authorize(proposal, signature)
    }
}

pub type BramkaAutoryzacji = dyn AuthorizationGate;

/// Production signature gate requiring valid Ed25519 signature.
#[derive(Default)]
pub struct SignatureGate;

pub type BramkaPodpisu = SignatureGate;
#[allow(non_upper_case_globals)]
pub const BramkaPodpisu: SignatureGate = SignatureGate;

impl AuthorizationGate for SignatureGate {
    fn authorize(&self, proposal: &UserOrderProposal, signature: &str) -> Result<(), SimonError> {
        if proposal.user_pubkey.trim().is_empty() {
            return Err(SimonError::ReceiptMismatch(
                "D110: missing user public key".into(),
            ));
        }
        if signature.trim().is_empty() {
            return Err(SimonError::ReceiptMismatch(
                "D110: missing user signature -> order cannot be broadcast".into(),
            ));
        }

        let digest = proposal.digest()?;

        let pk = simon_core::PublicKey::from_hex(&proposal.user_pubkey).map_err(|_| {
            SimonError::ReceiptMismatch("D110: user public key is not valid hex".into())
        })?;

        let bytes = hex::decode(signature.trim()).map_err(|_| {
            SimonError::ReceiptMismatch("D110: signature is not valid hex".into())
        })?;
        let len = bytes.len();
        let arr: [u8; 64] = bytes.try_into().map_err(|_| {
            SimonError::ReceiptMismatch(format!(
                "D110: Ed25519 signature must be 64 bytes, got {}",
                len
            ))
        })?;

        pk.verify_digest(&digest, &simon_core::Signature(arr))
            .map_err(|_| {
                SimonError::ReceiptMismatch(
                    "D110: user signature does NOT match proposal digest".into(),
                )
            })
    }
}

/// ACP Server — SIMON protocol client facing the swarm.
pub struct AcpServer {
    swarm: SwarmHandle,
    gate: Box<dyn AuthorizationGate>,
    active: HashMap<String, ActiveOrder>,
    pending_broadcast: Vec<JobOrder>,
}

/// State of an active order.
#[derive(Debug, Clone)]
pub struct ActiveOrder {
    pub order: JobOrder,
    pub accepted: Option<OrderAccepted>,
    pub phase: OrderPhase,
}

pub type AktywneZlecenie = ActiveOrder;

impl AcpServer {
    pub fn new(swarm: SwarmHandle, gate: Box<dyn AuthorizationGate>) -> Self {
        Self {
            swarm,
            gate,
            active: HashMap::new(),
            pending_broadcast: Vec::new(),
        }
    }

    /// Submits a user order proposal.
    pub fn submit(
        &mut self,
        proposal: UserOrderProposal,
        signature: &str,
    ) -> Result<String, SimonError> {
        self.gate.authorize(&proposal, signature)?;

        if proposal.fee_think == 0 {
            return Err(SimonError::ReceiptMismatch(
                "D83: fee 0 -> no THINK emission".into(),
            ));
        }

        let order = JobOrder::new(
            &proposal.model_hash,
            proposal.content.as_bytes(),
            proposal.fee_think,
            proposal.timeout_secs,
            &proposal.user_pubkey,
        )?;
        let order_id = order.order_id.clone();

        self.active.insert(
            order_id.clone(),
            ActiveOrder {
                order: order.clone(),
                accepted: None,
                phase: OrderPhase::Broadcast,
            },
        );
        self.pending_broadcast.push(order);
        Ok(order_id)
    }

    pub fn przyjmij(
        &mut self,
        proposal: UserOrderProposal,
        signature: &str,
    ) -> Result<String, SimonError> {
        self.submit(proposal, signature)
    }

    /// Broadcasts pending authorized orders to gossipsub.
    pub fn broadcast(&mut self) -> Result<usize, SimonError> {
        let mut n = 0;
        for order in self.pending_broadcast.drain(..) {
            let bytes = serde_json::to_vec(&order).map_err(|e| {
                SimonError::ReceiptMismatch(format!("order serialization: {e}"))
            })?;
            if let Err(e) = self.swarm.publish(TOPIC_JOB_ORDER, bytes) {
                return Err(SimonError::ReceiptMismatch(format!(
                    "order broadcast failed: {e}"
                )));
            }
            n += 1;
        }
        Ok(n)
    }

    pub fn rozglos(&mut self) -> Result<usize, SimonError> {
        self.broadcast()
    }

    /// Handles an incoming swarm event.
    pub fn handle_event(&mut self, event: &SimonEvent) -> Option<(String, OrderPhase)> {
        match event {
            SimonEvent::Message { topic, data, .. } if topic == TOPIC_ORDER_ACCEPTED => {
                let acc: OrderAccepted = serde_json::from_slice(data).ok()?;
                let order_id = acc.order_id.clone();
                if !self.active.contains_key(&order_id) {
                    return None;
                }
                let winner = {
                    let state = self.active.get(&order_id)?;
                    let mut candidates: Vec<OrderAccepted> = Vec::new();
                    if let Some(prev) = &state.accepted {
                        candidates.push(prev.clone());
                    }
                    candidates.push(acc.clone());
                    OrderAccepted::winner(&candidates).cloned()
                };
                let winner = winner?;
                let phase = OrderPhase::Accepted {
                    job_id: winner.job_id.clone(),
                };
                if let Some(state) = self.active.get_mut(&order_id) {
                    if matches!(state.phase, OrderPhase::Verifying | OrderPhase::Completed) {
                        return None;
                    }
                    state.accepted = Some(winner);
                    state.phase = phase.clone();
                }
                Some((order_id, phase))
            }
            SimonEvent::Message { topic, data, .. } if topic == TOPIC_ORDER_COMPLETED => {
                let done: OrderCompleted = serde_json::from_slice(data).ok()?;
                let order_id = done.order_id.clone();
                if !self.active.contains_key(&order_id) {
                    return None;
                }
                let phase = OrderPhase::Verifying;
                if let Some(state) = self.active.get_mut(&order_id) {
                    state.phase = phase.clone();
                }
                Some((order_id, phase))
            }
            _ => None,
        }
    }

    pub fn obsluz_event(&mut self, event: &SimonEvent) -> Option<(String, OrderPhase)> {
        self.handle_event(event)
    }

    /// Verifies completion and returns OrderResult.
    pub fn complete(
        &mut self,
        completed: &OrderCompleted,
        node_key: &simon_core::PublicKey,
        result_content: &str,
    ) -> Result<OrderResult, SimonError> {
        let state = self.active.get(&completed.order_id).ok_or_else(|| {
            SimonError::ReceiptMismatch(format!(
                "D106: result for unknown order {}",
                completed.order_id
            ))
        })?;
        let order = state.order.clone();
        let accepted = state.accepted.clone();

        if let Some(acc) = &accepted {
            if acc.job_id != completed.job_id {
                return Err(SimonError::ReceiptMismatch(format!(
                    "job_id: result={} accepted={}",
                    completed.job_id, acc.job_id
                )));
            }
            if acc.coordinator_pubkey != completed.coordinator_pubkey {
                return Err(SimonError::ReceiptMismatch(format!(
                    "coordinator: result={} race_winner={}",
                    completed.coordinator_pubkey, acc.coordinator_pubkey
                )));
            }
        }

        let expected_job = crate::transport::deterministic_job_id(&order.order_id);
        if completed.job_id != expected_job {
            return Err(SimonError::ReceiptMismatch(format!(
                "job_id does not match order_id: expected={} got={}",
                expected_job, completed.job_id
            )));
        }

        if completed.receipt.model_hash != order.model_hash {
            return Err(SimonError::ReceiptMismatch(format!(
                "model: receipt={} order={}",
                completed.receipt.model_hash, order.model_hash
            )));
        }

        crate::client_protocol::verify_completion(completed, &order.order_id, node_key)?;

        {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(result_content.as_bytes());
            let digest = hex::encode(h.finalize());
            if digest != completed.receipt.output_digest {
                return Err(SimonError::ReceiptMismatch(format!(
                    "output_digest: content={} receipt={}",
                    digest, completed.receipt.output_digest
                )));
            }
        }

        if let Some(state) = self.active.get_mut(&completed.order_id) {
            state.phase = OrderPhase::Completed;
        }

        Ok(OrderResult {
            order_id: completed.order_id.clone(),
            job_id: completed.job_id.clone(),
            content: result_content.to_string(),
            receipt: completed.receipt.clone(),
        })
    }

    pub fn zakoncz(
        &mut self,
        completed: &OrderCompleted,
        node_key: &simon_core::PublicKey,
        result_content: &str,
    ) -> Result<OrderResult, SimonError> {
        self.complete(completed, node_key, result_content)
    }

    /// Current phase of an order.
    pub fn phase(&self, order_id: &str) -> Option<&OrderPhase> {
        self.active.get(order_id).map(|a| &a.phase)
    }

    pub fn faza(&self, order_id: &str) -> Option<&OrderPhase> {
        self.phase(order_id)
    }

    /// Awaits order completion.
    pub async fn wait_for_completion(
        rx: &mut mpsc::UnboundedReceiver<SimonEvent>,
        order_id: &str,
        limit: Duration,
    ) -> Option<OrderCompleted> {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(SimonEvent::Message { topic, data, .. }))
                    if topic == TOPIC_ORDER_COMPLETED =>
                {
                    if let Ok(done) = serde_json::from_slice::<OrderCompleted>(&data) {
                        if done.order_id == order_id {
                            return Some(done);
                        }
                    }
                }
                Ok(Some(_)) => continue,
                Ok(None) | Err(_) => return None,
            }
        }
    }

    pub async fn czekaj_na_zakonczenie(
        rx: &mut mpsc::UnboundedReceiver<SimonEvent>,
        order_id: &str,
        limit: Duration,
    ) -> Option<OrderCompleted> {
        Self::wait_for_completion(rx, order_id, limit).await
    }
}
