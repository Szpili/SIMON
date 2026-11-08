//! SIMON Cryptography - Ed25519 (ed25519-dalek).
//!
//! Public key = node identity (reputation per OPERATOR).
//! Curve used EXCLUSIVELY for signatures - network security rests on crypto + stake.

use ed25519_dalek::{Signature as DalekSignature, Signer};
#[cfg(not(target_arch = "wasm32"))]
use rand::rngs::OsRng;

use crate::SimonError;

/// Keypair. In SIMON every node identity is bound to a signature keypair.
pub struct Keypair {
    inner: ed25519_dalek::SigningKey,
}

/// Public key (32 B) - node identity in the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct PublicKey(#[serde(with = "hex_bytes")] pub [u8; 32]);

/// Signature (64 B).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(#[serde(with = "hex_bytes64")] pub [u8; 64]);

impl Keypair {
    /// Generates a new keypair from system CSPRNG.
    ///
    /// Host-only: on wasm32-unknown-unknown system CSPRNG is unavailable without JS glue.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn generate() -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::generate(&mut OsRng),
        }
    }

    /// Reconstructs keypair from a 32-byte seed (for tests and deterministic replay).
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::from_bytes(seed),
        }
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.inner.verifying_key().to_bytes())
    }

    /// Signs content digest. We sign the DIGEST, not raw content -
    /// allowing the same API across receipts, jobs, and protocol messages.
    pub fn sign_digest(&self, digest_hex: &str) -> Signature {
        Signature(self.inner.sign(digest_hex.as_bytes()).to_bytes())
    }
}

impl PublicKey {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, SimonError> {
        let raw = hex::decode(s).map_err(|e| SimonError::Crypto(e.to_string()))?;
        let arr: [u8; 32] = raw
            .try_into()
            .map_err(|_| SimonError::Crypto("public key must be 32 bytes".into()))?;
        Ok(PublicKey(arr))
    }

    /// Verifies signature over content digest.
    pub fn verify_digest(&self, digest_hex: &str, sig: &Signature) -> Result<(), SimonError> {
        let key = ed25519_dalek::VerifyingKey::from_bytes(&self.0)
            .map_err(|e| SimonError::Crypto(e.to_string()))?;
        // Small-order keys could allow valid signatures without private keys.
        // Reject weak keys explicitly before verification.
        if key.is_weak() {
            return Err(SimonError::BadSignature);
        }
        let s = DalekSignature::from_bytes(&sig.0);
        // verify_strict enforces canonical s and R and rejects small order points.
        key.verify_strict(digest_hex.as_bytes(), &s)
            .map_err(|_| SimonError::BadSignature)
    }
}

// --- serde helpers: bytes as hex in JSON ---

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(b))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let raw = hex::decode(s).map_err(serde::de::Error::custom)?;
        raw.try_into().map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

mod hex_bytes64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(b))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let s = String::deserialize(d)?;
        let raw = hex::decode(s).map_err(serde::de::Error::custom)?;
        raw.try_into().map_err(|_| serde::de::Error::custom("expected 64 bytes"))
    }
}

use serde::{Deserialize, Serialize};
