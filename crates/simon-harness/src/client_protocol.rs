//! Client <-> Coordinator protocol (SIMON, transport: libp2p + gossipsub).
//!
//! Client BROADCASTS job order to the swarm — without targeting a specific coordinator.
//! This module contains ONLY the contract (types + rule validation), no networking.

use serde::{Deserialize, Serialize};

use simon_core::crypto::Keypair;
use simon_core::receipt::Receipt;
use simon_core::{content_digest, SimonError};

/// M0.4b: H(nonce || payload). Nonce prevents dictionary attacks on short prompts.
pub fn payload_digest_with_nonce(nonce: &str, payload: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(nonce.as_bytes());
    h.update(b"|");
    h.update(payload);
    hex::encode(h.finalize())
}

/// Compatibility alias for `payload_digest_with_nonce`.
pub fn payload_digest_z_nonce(nonce: &str, payload: &[u8]) -> String {
    payload_digest_with_nonce(nonce, payload)
}

/// Random order nonce (M0.2). 16 hex bytes.
fn new_nonce() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}

/// Gossipsub topic where clients broadcast job orders.
pub const TOPIC_JOB_ORDER: &str = "simon/v1/job-order";
/// Topic for order acceptance receipts.
pub const TOPIC_ORDER_ACCEPTED: &str = "simon/v1/order-accepted";
/// Topic for completed orders.
pub const TOPIC_ORDER_COMPLETED: &str = "simon/v1/order-completed";
/// Topic for node registration (model, max_ctx).
pub const TOPIC_NODE_REGISTER: &str = "simon/v1/node-register";

/// Proof of fee burn (D83 + D103).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BurnProof {
    /// Burning transaction hash on the THINK chain.
    pub tx_hash: String,
    /// Block height where transaction is confirmed.
    pub block_number: u64,
    /// Merkle proof that transaction belongs to the block.
    pub merkle_proof: Vec<String>,
    /// Nonce protecting against replay attacks.
    pub nonce: u64,
    /// Burn timestamp.
    pub burned_at: f64,
    /// Owner signature proving funds burned belong to caller.
    pub owner_signature: String,
    /// Whether this is a synthetic proof for MVP.
    pub synthetic: bool,
}

impl BurnProof {
    /// Synthetic proof for MVP.
    pub fn synthetic(order_id: &str, fee: u64) -> Self {
        Self {
            tx_hash: format!("synthetic:{}:{}", order_id, fee),
            block_number: 0,
            merkle_proof: Vec::new(),
            nonce: 0,
            burned_at: 0.0,
            owner_signature: String::new(),
            synthetic: true,
        }
    }

    /// Gate D83: proof must be verifiable.
    pub fn is_verifiable(&self) -> bool {
        !self.synthetic
            && !self.tx_hash.is_empty()
            && self.block_number > 0
            && !self.owner_signature.is_empty()
    }
}

/// User authorization for a job order (M0.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderAuthorization {
    /// Format version in signed content (M0.4a).
    pub v: u16,
    /// H(nonce || payload) — prompt text itself remains hidden from coordinator.
    pub payload_digest: String,
    pub model_hash: String,
    pub fee_think: u64,
    pub timeout_secs: u32,
    /// User nonce (M0.4b).
    pub nonce: String,
    /// Expiration timestamp in seconds (unix ts).
    #[serde(rename = "valid_until", alias = "wazne_do")]
    pub valid_until: u64,
    pub user_pubkey: String,
    /// User signature over `digest()` of the above fields.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
}

/// Current format version (M0.4a).
pub const FORMAT_V: u16 = 1;

/// Clock tolerance in seconds for expiration validation (M0.4b).
pub const CLOCK_TOLERANCE_SECS: u64 = 60;

impl OrderAuthorization {
    /// Authorization digest (signature excluded).
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        content_digest(&unsigned)
    }

    /// Signs authorization with user keypair.
    pub fn sign(mut self, keypair: &Keypair) -> Self {
        if let Ok(d) = self.digest() {
            self.signature = Some(hex::encode(keypair.sign_digest(&d).0));
        }
        self
    }

    /// Whether signature is valid for declared `user_pubkey`.
    pub fn verify_signature(&self) -> bool {
        let Some(sig_hex) = &self.signature else {
            return false;
        };
        let Ok(pk) = simon_core::PublicKey::from_hex(&self.user_pubkey) else {
            return false;
        };
        let Ok(d) = self.digest() else {
            return false;
        };
        let Ok(bytes) = hex::decode(sig_hex) else {
            return false;
        };
        let Ok(arr) = <[u8; 64]>::try_from(bytes.as_slice()) else {
            return false;
        };
        pk.verify_digest(&d, &simon_core::Signature(arr)).is_ok()
    }

    /// Whether authorization is still valid within clock tolerance.
    pub fn is_valid(&self, now_unix: u64) -> bool {
        now_unix <= self.valid_until + CLOCK_TOLERANCE_SECS
    }

    pub fn czy_wazna(&self, now_unix: u64) -> bool {
        self.is_valid(now_unix)
    }

    /// Whether authorization matches clear fields of the job order.
    pub fn matches_order(&self, order: &JobOrder) -> bool {
        self.payload_digest == order.payload_digest
            && self.model_hash == order.model_hash
            && self.fee_think == order.fee_think
            && self.timeout_secs == order.timeout_secs
            && self.user_pubkey == order.client_pubkey
            && self.nonce == order.nonce
    }

    pub fn zgodna_z(&self, order: &JobOrder) -> bool {
        self.matches_order(order)
    }
}

/// Client job order broadcast on TOPIC_JOB_ORDER.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobOrder {
    pub order_id: String,
    pub model_hash: String,
    pub payload_digest: String,
    pub fee_think: u64,
    pub burn: BurnProof,
    pub timeout_secs: u32,
    pub client_pubkey: String,
    pub nonce: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
    #[serde(rename = "authorization", alias = "autoryzacja", skip_serializing_if = "Option::is_none", default)]
    pub authorization: Option<OrderAuthorization>,
}

impl JobOrder {
    /// Creates an unsigned job order.
    pub fn new(
        model_hash: impl Into<String>,
        payload: &[u8],
        fee_think: u64,
        timeout_secs: u32,
        client_pubkey: impl Into<String>,
    ) -> Result<Self, SimonError> {
        let nonce = new_nonce();
        let payload_digest = payload_digest_with_nonce(&nonce, payload);
        let mut order = Self {
            burn: BurnProof::synthetic("", fee_think),
            order_id: String::new(),
            model_hash: model_hash.into(),
            payload_digest,
            fee_think,
            timeout_secs,
            client_pubkey: client_pubkey.into(),
            nonce,
            signature: None,
            authorization: None,
        };
        order.order_id = order.digest()?;
        let id = order.order_id.clone();
        order.burn = BurnProof::synthetic(&id, fee_think);
        Ok(order)
    }

    /// Checks whether `order_id` matches the computed digest.
    pub fn order_id_matches(&self) -> bool {
        match self.digest() {
            Ok(d) => d == self.order_id,
            Err(_) => false,
        }
    }

    pub fn order_id_sie_zgadza(&self) -> bool {
        self.order_id_matches()
    }

    /// Content digest of the order (signature and order_id excluded).
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        unsigned.order_id = String::new();
        unsigned.authorization = None;
        unsigned.burn = BurnProof {
            tx_hash: String::new(),
            block_number: 0,
            merkle_proof: Vec::new(),
            nonce: 0,
            burned_at: 0.0,
            owner_signature: String::new(),
            ..unsigned.burn
        };
        content_digest(&unsigned)
    }

    pub fn is_funded(&self) -> bool {
        self.fee_think > 0
    }

    pub fn has_verifiable_burn(&self) -> bool {
        self.burn.is_verifiable()
    }

    /// Verifies client authorization on coordinator side.
    pub fn verify_authorization(&self, now_unix: u64) -> Result<String, SimonError> {
        let Some(auth) = &self.authorization else {
            return Err(SimonError::ReceiptMismatch(
                "D110: order without user authorization".into(),
            ));
        };
        if auth.v != FORMAT_V {
            return Err(SimonError::ReceiptMismatch(format!(
                "M0.4a: unknown format version {} (expected {})",
                auth.v, FORMAT_V
            )));
        }
        if !auth.matches_order(self) {
            return Err(SimonError::ReceiptMismatch(
                "M0.4c: order fields do NOT match authorization (substitution attempt)".into(),
            ));
        }
        if !auth.verify_signature() {
            return Err(SimonError::ReceiptMismatch(
                "D110: user signature does not match authorization".into(),
            ));
        }
        if !auth.is_valid(now_unix) {
            return Err(SimonError::ReceiptMismatch(format!(
                "M0.4b: authorization expired (valid_until={}, now={})",
                auth.valid_until, now_unix
            )));
        }
        auth.digest()
    }
}

/// Registry of spent authorizations (replay protection).
#[derive(Debug, Clone, Default)]
pub struct SpentAuthorizationRegistry {
    spent: std::collections::HashSet<String>,
}

impl SpentAuthorizationRegistry {
    /// Checks and consumes authorization digest. Replays return error.
    pub fn consume(&mut self, auth_digest: &str) -> Result<(), SimonError> {
        if !self.spent.insert(auth_digest.to_string()) {
            return Err(SimonError::ReceiptMismatch(
                "M0.4b: authorization already consumed (replay of the same signature)".into(),
            ));
        }
        Ok(())
    }
}

/// Order accepted confirmation broadcast on TOPIC_ORDER_ACCEPTED.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderAccepted {
    pub order_id: String,
    pub job_id: String,
    pub coordinator_pubkey: String,
    pub vrf_seed: String,
    pub vrf_block: u64,
    pub estimated_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
}

impl OrderAccepted {
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        content_digest(&unsigned)
    }

    pub fn sign(mut self, keypair: &Keypair) -> Self {
        if let Ok(d) = self.digest() {
            self.signature = Some(hex::encode(keypair.sign_digest(&d).0));
        }
        self
    }

    pub fn verify_signature(&self) -> bool {
        let Some(sig_hex) = &self.signature else {
            return false;
        };
        let Ok(pk) = simon_core::PublicKey::from_hex(&self.coordinator_pubkey) else {
            return false;
        };
        let Ok(d) = self.digest() else {
            return false;
        };
        let Ok(bytes) = hex::decode(sig_hex) else {
            return false;
        };
        let Ok(arr): Result<[u8; 64], _> = bytes.try_into() else {
            return false;
        };
        pk.verify_digest(&d, &simon_core::Signature(arr)).is_ok()
    }

    /// Resolves race when multiple coordinators accept the same order.
    pub fn winner(candidates: &[OrderAccepted]) -> Option<&OrderAccepted> {
        candidates.iter().min_by(|a, b| {
            Self::ranking(a)
                .cmp(&Self::ranking(b))
                .then_with(|| a.job_id.cmp(&b.job_id))
        })
    }

    pub fn ranking_internal(c: &OrderAccepted) -> String {
        Self::ranking(c)
    }

    fn ranking(c: &OrderAccepted) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(c.order_id.as_bytes());
        h.update(b"|");
        h.update(c.coordinator_pubkey.as_bytes());
        hex::encode(h.finalize())
    }
}

/// Order completion broadcast on TOPIC_ORDER_COMPLETED.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderCompleted {
    pub order_id: String,
    pub job_id: String,
    pub node_id: String,
    pub receipt: Receipt,
    pub coordinator_pubkey: String,
}

/// Verifies completed order locally.
pub fn verify_completion(
    completed: &OrderCompleted,
    expected_order_id: &str,
    registered_node_key: &simon_core::PublicKey,
) -> Result<(), SimonError> {
    if completed.order_id != expected_order_id {
        return Err(SimonError::ReceiptMismatch(format!(
            "order_id: received={} expected={}",
            completed.order_id, expected_order_id
        )));
    }

    completed.receipt.verify_for_node(
        &completed.job_id,
        &completed.node_id,
        registered_node_key,
    )?;

    Ok(())
}

/// Static list of trusted coordinators.
#[derive(Debug, Clone, Default)]
pub struct CoordinatorRegistry {
    keys: std::collections::HashSet<String>,
}

impl CoordinatorRegistry {
    pub fn with_keys(keys: &[String]) -> Self {
        Self {
            keys: keys.iter().cloned().collect(),
        }
    }

    pub fn contains(&self, pubkey_hex: &str) -> bool {
        self.keys.contains(pubkey_hex)
    }

    pub fn admit<'a>(
        &self,
        candidates: &'a [OrderAccepted],
        order_id: &str,
    ) -> Vec<&'a OrderAccepted> {
        candidates
            .iter()
            .filter(|c| c.order_id == order_id)
            .filter(|c| self.contains(&c.coordinator_pubkey))
            .filter(|c| c.verify_signature())
            .collect()
    }

    pub fn winner_registered<'a>(
        &self,
        candidates: &'a [OrderAccepted],
        order_id: &str,
    ) -> Option<&'a OrderAccepted> {
        let admitted = self.admit(candidates, order_id);
        admitted.into_iter().min_by(|a, b| {
            OrderAccepted::ranking_internal(a)
                .cmp(&OrderAccepted::ranking_internal(b))
                .then_with(|| a.job_id.cmp(&b.job_id))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(_id: &str, fee: u64) -> JobOrder {
        JobOrder::new("sha256:model", b"prompt", fee, 300, "aa".repeat(32)).unwrap()
    }

    #[test]
    fn funded_order_passes_gate() {
        assert!(order("o1", 10).is_funded());
    }

    #[test]
    fn zero_fee_order_fails_d83_gate() {
        assert!(!order("o1", 0).is_funded(), "D83: zero fee yields no emissions");
    }

    #[test]
    fn synthetic_burn_is_not_verifiable() {
        let o = order("o1", 10);
        assert!(!o.has_verifiable_burn(), "MVP proof is not production ready");
        assert!(o.burn.synthetic, "must be explicitly synthetic");
    }

    #[test]
    fn payload_digest_is_stable_within_order() {
        let a = order("o1", 10);
        assert_eq!(a.payload_digest, a.payload_digest);
        assert_eq!(a.payload_digest.len(), 64, "sha256 hex");
        let b = order("o1", 10);
        assert_ne!(
            a.payload_digest, b.payload_digest,
            "nonce in hash: same prompt yields different digest"
        );
    }

    #[test]
    fn signature_excluded_from_order_digest() {
        let mut o = order("o1", 10);
        let before = o.digest().unwrap();
        o.signature = Some("deadbeef".into());
        assert_eq!(before, o.digest().unwrap());
    }

    #[test]
    fn gossipsub_race_resolved_deterministically() {
        let mk = |pk: &str| OrderAccepted {
            order_id: "o1".into(),
            job_id: "j1".into(),
            coordinator_pubkey: pk.to_string(),
            vrf_seed: "seed".into(),
            vrf_block: 7,
            estimated_ms: 100,
            signature: None,
        };
        let a = mk("ff");
        let b = mk("aa");
        let candidates = [a.clone(), b.clone()];
        let winner = OrderAccepted::winner(&candidates).unwrap();
        let reversed = [b.clone(), a.clone()];
        let winner2 = OrderAccepted::winner(&reversed).unwrap();
        assert_eq!(
            winner.coordinator_pubkey, winner2.coordinator_pubkey,
            "winner cannot depend on arrival order"
        );
        let ranking = |pk: &str| {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(b"o1|");
            h.update(pk.as_bytes());
            hex::encode(h.finalize())
        };
        let (ra, rb) = (ranking("ff"), ranking("aa"));
        let expected = if ra < rb { "ff" } else { "aa" };
        assert_eq!(
            winner.coordinator_pubkey, expected,
            "min H(order_id || pubkey) wins"
        );
    }

    #[test]
    fn vanity_pubkey_does_not_hijack_race() {
        let mk = |pk: &str| OrderAccepted {
            order_id: "o1".into(),
            job_id: "j1".into(),
            coordinator_pubkey: pk.to_string(),
            vrf_seed: "seed".into(),
            vrf_block: 7,
            estimated_ms: 100,
            signature: None,
        };
        let attack = mk("0000000000000000000000000000000000000000000000000000000000000000");
        let ordinary = mk("ff");
        let candidates = [attack.clone(), ordinary.clone()];
        let winner = OrderAccepted::winner(&candidates).unwrap();
        let ranking = |pk: &str| {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(pk.as_bytes());
            hex::encode(h.finalize())
        };
        let expected = if ranking("0000000000000000000000000000000000000000000000000000000000000000")
            < ranking("ff")
        {
            "0000000000000000000000000000000000000000000000000000000000000000"
        } else {
            "ff"
        };
        assert_eq!(winner.coordinator_pubkey, expected);
    }
}
