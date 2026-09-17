//! SIMON harness — warstwa klienta: zlecanie pracy do klastra.
//!
//! To jest miejsce, gdzie harness gada z koordynatorem. Protokół node↔koordynator
//! istnieje w `~/DC`; TU jest brakująca trzecia warstwa: **klient → koordynator**.
//!
//! Decyzje, które realizuje:
//!   - D83  klient PŁACI, a opłata jest SPALONA → kopanie z własnych promptów
//!          nie opłaca się (spalasz tyle, ile zarabiasz)
//!   - D84  zadanie przydziela LOSOWANIE (VRF) — klient nie wybiera node'a
//!   - D11  użytkownik wybiera MODEL, nie maszyny

use serde::{Deserialize, Serialize};

use simon_core::SimonError;

pub mod acp_server;
pub mod request_response;
pub mod client_protocol;
pub mod transport;

pub use client_protocol::{
    verify_completion, BurnProof, JobOrder, OrderAccepted, OrderCompleted, TOPIC_JOB_ORDER,
    TOPIC_ORDER_ACCEPTED, TOPIC_ORDER_COMPLETED,
};
pub use transport::{deterministic_job_id, spawn_swarm, SimonEvent, SwarmHandle};
pub use acp_server::{
    AcpServer, BramkaAutoryzacji, BramkaPodpisu, FazaZlecenia, SessionId, WynikZlecenia,
    ZlecenieUzytkownika,
};

// UWAGA: `JobOrder`/`OrderAccepted` w tym pliku są STARSZĄ wersją roboczą.
// Kanonem jest `client_protocol` (gossipsub, D100-D104). Poniższe typy czekają
// na usunięcie — patrz ARCH-2026-09-16-interfejs-klient.md.

/// Zlecenie pracy od klienta (harness/UI) do koordynatora.
///
/// **PRZESTARZAŁE** — kanonem jest `client_protocol::JobOrder` (gossipsub, D100-D104).
/// Ten typ zostaje tymczasowo, żeby `Harness` się kompilował; do usunięcia.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyJobOrder {
    pub order_id: String,
    /// Wybrany model (D11 — user wybiera, na czym liczy).
    pub model_hash: String,
    /// Ciało zadania. NIE trafia do node'a w całości — node widzi aktywacje
    /// (RULES #1: prywatność przez architekturę, nie przez szyfrowanie promptu).
    pub payload: serde_json::Value,
    /// Ile klient płaci. Opłata jest SPALONA (D83) — nie idzie do kopacza.
    pub fee_think: u64,
    /// Maksymalny czas oczekiwania w sekundach.
    pub timeout_secs: u64,
}

/// Potwierdzenie zapłaty + przyjęcia zlecenia do kolejki.
///
/// **PRZESTARZAŁE** — kanonem jest `client_protocol::OrderAccepted`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyOrderAccepted {
    pub order_id: String,
    /// Dowód spalenia opłaty. Bez tego nie ma emisji (D83).
    pub burn_proof: String,
    /// Ziarno losowania VRF — klient może zweryfikować, że przydział był losowy (D84).
    pub vrf_seed: String,
}

/// Harness: kolejkuje zlecenia i pilnuje, żeby żadne nie poszło bez spalonej opłaty.
///
/// Ta warstwa jest CELOWO minimalna (wzorzec karpathy: najmniejsza rzecz, która
/// działa). Sieć dochodzi osobno — najpierw kontrakt, potem transport.
#[derive(Default)]
pub struct Harness {
    /// Zlecenia przyjęte do kolejki: order_id -> zlecenie.
    pub queue: std::collections::HashMap<String, LegacyJobOrder>,
    /// Ślad spaleń: order_id -> dowód. Audit trail (D81: emisja ≠ bezpieczeństwo).
    pub burns: std::collections::HashMap<String, String>,
}

impl Harness {
    pub fn new() -> Self {
        Self::default()
    }

    /// Przyjmuje zlecenie. Bramka D83: bez opłaty > 0 nie ma przyjęcia.
    ///
    /// To jest realizacja zasady „kopanie z własnych promptów się nie opłaca":
    /// jeśli chcesz THINK, musisz komuś za pracę zapłacić i tę opłatę spalić.
    pub fn order(&mut self, order: LegacyJobOrder) -> Result<LegacyOrderAccepted, SimonError> {
        if order.fee_think == 0 {
            return Err(SimonError::ReceiptMismatch(
                "opłata 0 → brak emisji (D83): bez spalonej opłaty nie ma THINK".into(),
            ));
        }
        if self.queue.contains_key(&order.order_id) {
            return Err(SimonError::ReceiptMismatch(format!(
                "zlecenie {} już w kolejce",
                order.order_id
            )));
        }

        // W realnej implementacji: podpis klienta + transakcja spalenia w łańcuchu.
        // Tu zapisujemy deterministyczny ślad, żeby kontrakt był testowalny.
        let burn_proof = format!("burn:{}:{}", order.order_id, order.fee_think);
        let vrf_seed = format!("vrf:{}", order.order_id);

        self.queue.insert(order.order_id.clone(), order.clone());
        self.burns.insert(order.order_id.clone(), burn_proof.clone());

        Ok(LegacyOrderAccepted {
            order_id: order.order_id,
            burn_proof,
            vrf_seed,
        })
    }

    /// Czy zlecenie ma spaloną opłatę? (Bramka emisji — D83.)
    pub fn has_burn(&self, order_id: &str) -> bool {
        self.burns.contains_key(order_id)
    }
}
