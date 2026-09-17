//! Receipt wykonania — format ZAMROŻONY (odwracalność: wysoka).
//!
//! Naprawa F3 (red-team RT1, `~/DC`): „complete() nie waliduje podpisu"
//! → 100% rozliczeń do podrobienia, wykrywalność 0%. Łamało D12 i D18.
//!
//! Trzy bramki weryfikacji (wariant C):
//!   1. podpis ważny pod kluczem z receiptu,
//!   2. klucz z receiptu == klucz ZAREJESTROWANY dla tego node'a,
//!   3. receipt dotyczy TEGO joba i TEGO node'a (zgodność pól).
//!
//! Bez bramki (2) atakujący podpisałby własnym kluczem i wstawił swój pubkey —
//! podpis byłby „ważny", ale nie byłby dowodem na zarejestrowanego node'a.

use serde::{Deserialize, Serialize};

use crate::crypto::{Keypair, PublicKey, Signature};
use crate::{content_digest, SimonError};

/// Precyzja obliczeń (D67 pkt 1: jedna kwantyzacja per klaster).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Bf16,
    Fp16,
    Fp32,
    Fp8,
    Fp4,
}

/// Receipt jest tym, co node składa jako dowód wykonania pracy.
///
/// Pole `signature` NIE wchodzi do odcisku treści — podpis podpisuje resztę.
/// Pole `signer` JEST częścią treści (mówi KTO podpisał) i musi zgadzać się
/// z kluczem zarejestrowanym w rejestrze koordynatora.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub job_id: String,
    pub node_id: String,
    /// Hash modelu (D67 pkt 1: niezgodny = inny klaster).
    pub model_hash: String,
    /// Wersja runtime (D67 pkt 2: niezgodna = nie wchodzi do puli).
    pub runtime: String,
    pub precision: Precision,
    /// LSH aktywacji wg TopLoc (258 B / 32 tokeny, arXiv 2501.16007).
    pub activation_hash: String,
    /// Odcisk wyniku.
    pub output_digest: String,
    /// Czas startu w MIKROSEKUNDACH (u64).
    ///
    /// NAPRAWA (2026-09-17): wcześniej `f64`. Float nie ma gwarantowanego
    /// roundtripu między formatami — JSON i CBOR serializują go inaczej,
    /// więc SHA-256 z treści wychodził INNY po przejściu przez transport
    /// i podpis przestawał pasować (`Err(BadSignature)`).
    /// Podpisywana treść NIE MOŻE zawierać floatów.
    pub started_at_us: u64,
    pub finished_at_us: u64,
    /// Klucz publiczny node'a (część treści — podlega podpisowi).
    pub signer: PublicKey,
    /// Podpis nad odciskiem treści. Wyłączony z odcisku.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<Signature>,
}

impl Receipt {
    /// Odcisk treści receiptu. Podpis nie wchodzi do odcisku.
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        content_digest(&unsigned)
    }

    /// Podpisuje receipt kluczem node'a. Ustawia `signer` z klucza.
    pub fn sign(mut self, keypair: &Keypair) -> Result<Self, SimonError> {
        self.signer = keypair.public();
        let digest = self.digest()?;
        self.signature = Some(keypair.sign_digest(&digest));
        Ok(self)
    }

    /// Weryfikuje podpis pod kluczem ZAWIARTYM w receipcie.
    ///
    /// To NIE wystarcza do przyjęcia receiptu — koordynator musi jeszcze
    /// sprawdzić, że `signer` to klucz zarejestrowany (patrz `verify_for_node`).
    pub fn verify_self(&self) -> Result<(), SimonError> {
        let sig = self.signature.as_ref().ok_or(SimonError::BadSignature)?;
        let digest = self.digest()?;
        self.signer.verify_digest(&digest, sig)
    }

    /// Wariant C — pełna weryfikacja wobec zarejestrowanego klucza node'a.
    ///
    /// Trzy bramki. Każda musi przejść. To zamyka F3.
    pub fn verify_for_node(
        &self,
        expected_job_id: &str,
        expected_node_id: &str,
        registered_key: &PublicKey,
    ) -> Result<(), SimonError> {
        // Bramka 1: podpis ważny pod kluczem z receiptu.
        self.verify_self()?;

        // Bramka 2: klucz z receiptu == klucz zarejestrowany.
        if &self.signer != registered_key {
            return Err(SimonError::UnregisteredKey);
        }

        // Bramka 3: receipt dotyczy tego joba i tego node'a.
        if self.job_id != expected_job_id {
            return Err(SimonError::ReceiptMismatch(format!(
                "job_id: receipt={} oczekiwano={}",
                self.job_id, expected_job_id
            )));
        }
        if self.node_id != expected_node_id {
            return Err(SimonError::ReceiptMismatch(format!(
                "node_id: receipt={} oczekiwano={}",
                self.node_id, expected_node_id
            )));
        }
        Ok(())
    }

    /// Czas wykonania w sekundach (z mikrosekund). TopLoc §5: walidacja to
    /// jeden prefill, więc jest rząd wielkości tańsza niż generacja.
    pub fn elapsed_secs(&self) -> f64 {
        (self.finished_at_us.saturating_sub(self.started_at_us)) as f64 / 1_000_000.0
    }
}
