//! SIMON core — shared types between harness, coordinator, and node.
//!
//! Pure verification and domain primitives — zero networking, zero I/O.

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub mod cap;
pub mod crypto;
pub mod flow;
#[cfg(not(target_arch = "wasm32"))]
pub mod identity;
pub mod limits;
pub mod m3;
pub mod measurement;
pub mod observation;
pub mod protocol;
pub mod receipt;
#[cfg(not(target_arch = "wasm32"))]
pub mod registry;
pub mod web;

pub use crypto::{Keypair, PublicKey, Signature};
pub use receipt::Receipt;

/// Protocol version string.
pub const PROTOCOL_VERSION: &str = "simon/v1";

#[derive(Debug, Error)]
pub enum SimonError {
    #[error("invalid signature")]
    BadSignature,
    #[error("receipt does not match the job: {0}")]
    ReceiptMismatch(String),
    #[error("key not registered")]
    UnregisteredKey,
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("crypto error: {0}")]
    Crypto(String),
}

/// Content digest - deterministic. Two identical inputs yield identical digest.
///
/// Canonical representation: JSON with sorted keys and no whitespace. This ensures
/// signature portability across implementations (Rust <-> Python).
/// Note: serialized via `serde_json::to_string` with BTreeMap key ordering.
pub fn content_digest<T: Serialize>(value: &T) -> Result<String, SimonError> {
    // ponytail: serde_json wystarcza, dopoki jestesmy jedyna implementacja;
    // przy drugiej -> RFC 8785.
    let canonical = serde_json::to_string(value)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}
