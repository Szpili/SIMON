//! Kryptografia SIMON — Ed25519 (plan §1: `ed25519-dalek`, NIE secp256k1).
//!
//! Klucz publiczny = tożsamość node'a (D15: reputacja per OPERATOR).
//! D81: krzywa służy WYŁĄCZNIE do podpisów — bezpieczeństwo łańcucha stoi
//! na krypto + stake, nigdy na ilości policzonej inferencji.

use ed25519_dalek::{Signature as DalekSignature, Signer, Verifier};
use rand::rngs::OsRng;

use crate::SimonError;

/// Para kluczy. W SIMON nie ma portfeli bez podpisów (D12), a nie ma podpisów
/// bez trwałej tożsamości.
pub struct Keypair {
    inner: ed25519_dalek::SigningKey,
}

/// Klucz publiczny (32 B) — tożsamość node'a w rejestrze.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct PublicKey(#[serde(with = "hex_bytes")] pub [u8; 32]);

/// Podpis (64 B).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(#[serde(with = "hex_bytes64")] pub [u8; 64]);

impl Keypair {
    /// Generuje nową parę kluczy z CSPRNG systemowego.
    pub fn generate() -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::generate(&mut OsRng),
        }
    }

    /// Odtwarza parę z 32-bajtowego ziarna (testy, deterministyczny replay).
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::from_bytes(seed),
        }
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.inner.verifying_key().to_bytes())
    }

    /// Podpisuje odcisk treści. Podpisujemy ODCISK, nie surową treść —
    /// dzięki temu to samo API obsługuje receipty, joby i wiadomości protokołu.
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
            .map_err(|_| SimonError::Crypto("klucz publiczny musi mieć 32 bajty".into()))?;
        Ok(PublicKey(arr))
    }

    /// Weryfikuje podpis nad odciskiem treści.
    pub fn verify_digest(&self, digest_hex: &str, sig: &Signature) -> Result<(), SimonError> {
        let key = ed25519_dalek::VerifyingKey::from_bytes(&self.0)
            .map_err(|e| SimonError::Crypto(e.to_string()))?;
        let s = DalekSignature::from_bytes(&sig.0);
        key.verify(digest_hex.as_bytes(), &s)
            .map_err(|_| SimonError::BadSignature)
    }
}

// --- serde helpery: bajty jako hex w JSON (czytelne w receipcie) ---

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(b))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let raw = hex::decode(s).map_err(serde::de::Error::custom)?;
        raw.try_into().map_err(|_| serde::de::Error::custom("oczekiwano 32 bajtów"))
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
        raw.try_into().map_err(|_| serde::de::Error::custom("oczekiwano 64 bajtów"))
    }
}

use serde::{Deserialize, Serialize};
