//! Obserwacja klienta — czas ZMIERZONY, nie zadeklarowany.
//!
//! Rozdzielenie ról jest tu istotą, nie formalnością:
//!
//! ```text
//! ExecutionReceipt  — podpisuje NODE
//! ClientObservation — podpisuje KLIENT
//! ```
//!
//! Node nie może podpisać czasu obserwowanego przez klienta, bo ten czas
//! powstaje dopiero po wysłaniu odpowiedzi. Dlatego obserwacja NIE wchodzi do
//! receiptu — jest osobnym artefaktem, powiązanym z nim przez `receipt_hash`.
//!
//! Pola `ttft_ms` i `gen_ms` w receipcie to `node_declared_*`. Podpis
//! gwarantuje wyłącznie, że node takie wartości zadeklarował — nie że
//! poprawnie je zmierzył. **Nie wolno ich używać do rozliczeń, slasha,
//! rankingu wydajności ani rozstrzygania sporów.**

use serde::{Deserialize, Serialize};

use crate::{
    content_digest,
    crypto::{Keypair, PublicKey, Signature},
    SimonError,
};

/// Separator domeny — ten sam hash nie może znaczyć czegoś innego gdzie indziej.
pub const DOMENA_OBSERWACJI: &str = "SIMON/OBSERVATION/v1";

/// Czym szło zlecenie. Ma znaczenie przy porównywaniu czasów.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    /// libp2p request-response, jeden pakiet odpowiedzi (bez streamingu).
    Libp2pRequestResponse,
}

/// Czas mierzony przez klienta, zegarem MONOTONICZNYM.
///
/// Celowo NIE liczymy różnicy absolutnych znaczników z dwóch maszyn —
/// synchronizacja zegarów dołożyłaby błąd, którego nie musimy mieć.
///
/// Granice pomiaru (ustalone przed implementacją):
/// - `observed_time_to_first_event_ms`: od wysłania pierwszego bajtu żądania do
///   odebrania pierwszego poprawnego zdarzenia zawierającego wyjście.
///   **`None` bez streamingu** — czas otrzymania całej odpowiedzi to NIE jest
///   TTFT i nie wolno go tak nazywać.
/// - `observed_time_to_complete_ms`: od wysłania pierwszego bajtu żądania do
///   odebrania I ZWERYFIKOWANIA kompletnego wyjścia.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientObservationV1 {
    pub receipt_hash: String,
    pub job_id: String,
    pub client_pubkey: PublicKey,

    pub observed_time_to_first_event_ms: Option<u64>,
    pub observed_time_to_complete_ms: u64,

    /// Rozbicie na poziomy pewności — widać, ile kosztuje każdy kolejny.
    pub network_complete_ms: u64,
    pub output_bound_ms: u64,
    /// Dopiero po wdrożeniu weryfikatora. Dziś zawsze `None`.
    pub execution_audit_complete_ms: Option<u64>,

    pub transport: Transport,
    pub streamed: bool,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub client_signature: Option<Signature>,
}

impl ClientObservationV1 {
    /// Odcisk treści obserwacji. Podpis nie wchodzi do odcisku.
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut bez_podpisu = self.clone();
        bez_podpisu.client_signature = None;
        content_digest(&serde_json::json!({
            "domena": DOMENA_OBSERWACJI,
            "obserwacja": bez_podpisu,
        }))
    }

    pub fn sign(mut self, klucz: &Keypair) -> Result<Self, SimonError> {
        let d = self.digest()?;
        self.client_signature = Some(klucz.sign_digest(&d));
        Ok(self)
    }

    /// Czy obserwacja jest spójna z kluczem, który się pod nią podpisał.
    pub fn verify_self(&self) -> Result<(), SimonError> {
        let podpis = self.client_signature.as_ref().ok_or(SimonError::BadSignature)?;
        let d = self.digest()?;
        self.client_pubkey.verify_digest(&d, podpis)
    }
}

#[cfg(test)]
mod testy {
    use super::*;

    fn przykladowa(klucz: &Keypair) -> ClientObservationV1 {
        ClientObservationV1 {
            receipt_hash: "abc".into(),
            job_id: "job-1".into(),
            client_pubkey: klucz.public(),
            observed_time_to_first_event_ms: None,
            observed_time_to_complete_ms: 1234,
            network_complete_ms: 1200,
            output_bound_ms: 1234,
            execution_audit_complete_ms: None,
            transport: Transport::Libp2pRequestResponse,
            streamed: false,
            client_signature: None,
        }
    }

    #[test]
    fn podpisana_obserwacja_weryfikuje_sie() {
        let k = Keypair::generate();
        let o = przykladowa(&k).sign(&k).expect("podpis");
        assert!(o.verify_self().is_ok());
    }

    #[test]
    fn zmieniony_czas_lamie_podpis() {
        let k = Keypair::generate();
        let mut o = przykladowa(&k).sign(&k).expect("podpis");
        o.observed_time_to_complete_ms = 5;
        assert!(o.verify_self().is_err(), "czas jest czescia podpisanej tresci");
    }

    #[test]
    fn bez_streamingu_nie_ma_ttft() {
        let k = Keypair::generate();
        let o = przykladowa(&k);
        assert!(!o.streamed);
        assert!(
            o.observed_time_to_first_event_ms.is_none(),
            "czas calej odpowiedzi to NIE jest TTFT"
        );
    }
}
