//! SIMON core — typy współdzielone przez harness i node.
//!
//! Ten crate jest wspólnym językiem między `~/SIMON` (harness/UI) a `~/DC` (miner).
//! Zawiera WYŁĄCZNIE typy i weryfikację — zero sieci, zero I/O.
//!
//! Decyzje, które ten kod realizuje:
//!   - D81  emisja ≠ wartość ≠ bezpieczeństwo (bezpieczeństwo = krypto + stake)
//!   - D83  zapłata spalona jako warunek emisji THINK
//!   - D84  losowanie zadania (VRF), klient nie wybiera node'a
//!   - D12  podpisy (Ed25519 wg PLAN §1)
//!   - D67  jakość tokena: model_hash + runtime + precision w receipcie

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub mod crypto;
pub mod obserwacja;
pub mod pomiar;
pub mod rejestr;
pub mod tozsamosc;
pub mod receipt;

pub use crypto::{Keypair, PublicKey, Signature};
pub use receipt::Receipt;

/// Wersja protokołu. Część nieodwracalnego kontraktu — zmiana = nowa sieć.
pub const PROTOCOL_VERSION: &str = "simon/v1";

#[derive(Debug, Error)]
pub enum SimonError {
    #[error("podpis nieprawidłowy")]
    BadSignature,
    #[error("receipt niezgodny z jobem: {0}")]
    ReceiptMismatch(String),
    #[error("klucz nie zarejestrowany")]
    UnregisteredKey,
    #[error("błąd serializacji: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("błąd kryptograficzny: {0}")]
    Crypto(String),
}

/// Odcisk treści — deterministyczny. Dwa identyczne wejścia = ten sam odcisk.
///
/// Kanoniczność: JSON z posortowanymi kluczami, bez spacji. To jest jedyna
/// dopuszczalna metoda liczenia odcisku — inaczej podpis nie jest przenośny
/// między implementacjami (Rust ↔ Python).
/// UWAGA co do slowa "kanoniczny": to jest `serde_json::to_string`, a NIE
/// kanonizacja w sensie RFC 8785 (JCS). Wynik jest powtarzalny dla TEJ
/// implementacji (mapy `serde_json` to `BTreeMap`, wiec klucze sa posortowane;
/// floaty sa w podpisywanej tresci zakazane od 2026-09-17), ale inna
/// implementacja protokolu moze policzyc inny bajt-strumien dla tych samych
/// danych. Do interoperacyjnosci trzeba bedzie JCS albo formatu binarnego
/// o ustalonej kolejnosci pol — zapisane jako dlug, nie zalatwione.
pub fn content_digest<T: Serialize>(value: &T) -> Result<String, SimonError> {
    // ponytail: serde_json wystarcza, dopoki jestesmy jedyna implementacja;
    // przy drugiej -> RFC 8785.
    let canonical = serde_json::to_string(value)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}
