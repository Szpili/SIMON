//! SIMON node — strona node'a. Miner (`~/DC`) gada z tym przez protokół.
//!
//! Na razie WYŁĄCZNIE rejestr node'ów z kluczami publicznymi (wariant C dla F3)
//! i przyjmowanie wyników z weryfikacją receiptu. Zero sieci — najpierw kontrakt.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use simon_core::receipt::Receipt;
use simon_core::{PublicKey, SimonError, PROTOCOL_VERSION};

/// Stan node'a w rejestrze koordynatora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeState {
    Registered,
    Healthy,
    Busy,
    Degraded,
    Evicted,
}

/// Deklaracja mocy. `declared` = co node mówi, `attested` = co koordynator zmierzył.
///
/// RULES #2: do PRZYDZIAŁU służy wyłącznie `attested`. Bez tego sybil wygrywa.
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

/// Rejestracja node'a. `pubkey` deklarowany RAZ — tożsamość node'a = ten klucz (D15).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterMsg {
    pub node_id: String,
    pub protocol: String,
    pub capability: Capability,
    pub region: String,
    /// Wariant C: klucz publiczny wchodzi do rejestracji.
    pub pubkey: PublicKey,
}

impl RegisterMsg {
    pub fn new(node_id: impl Into<String>, pubkey: PublicKey, capability: Capability) -> Self {
        Self {
            node_id: node_id.into(),
            protocol: PROTOCOL_VERSION.to_string(),
            capability,
            region: "unknown".to_string(),
            pubkey,
        }
    }
}

/// Wynik joba przesłany przez node.
///
/// D101 (konsylium, gemma): wynik dla klienta musi nieść PEŁNY `Receipt` —
/// inaczej klient nie zweryfikuje D67 (model_hash/precision/activation_hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: String,
    pub node_id: String,
    pub ok: bool,
    pub output: Option<serde_json::Value>,
    pub receipt: Option<Receipt>,
    pub error: Option<String>,
}

impl JobResult {
    /// Sukces z podpisanym receiptem.
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
        }
    }

    /// Porażka — receiptu nie wymagamy, nie ma czego rozliczać.
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
        }
    }
}

/// Rejestr node'ów + jobów. Deterministyczny, bez sieci — testowalny bez I/O.
#[derive(Default)]
pub struct Registry {
    pub nodes: HashMap<String, RegisterMsg>,
    pub states: HashMap<String, NodeState>,
    /// Joby w toku: job_id -> node_id, do którego przydzielono.
    pub assigned: HashMap<String, String>,
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

    /// C (2026-09-17): kandydaci dopasowani po MODELU i LIMICIE KONTEKSTU.
    ///
    /// Filtr działa **PRZED** losowaniem — tak samo jak `RejestrKoordynatorow::dopusc()`
    /// filtruje przed rankingiem. Bez tego zlecenie na `bielik-awq` mogło trafić
    /// na node z `qwen3.8-27b`, a rozjazd łapała dopiero bramka D67 klienta —
    /// PO policzeniu i PO zapłacie.
    ///
    /// `wymagany_ctx = 0` znaczy „nie sprawdzaj kontekstu".
    /// Pusta lista = jawna odmowa (nikt nie ma modelu / zmieści kontekstu),
    /// a NIE cichy wybór losowego node'a.
    pub fn kandydaci_dla_modelu(&self, model_hash: &str, wymagany_ctx: u32) -> Vec<String> {
        let mut out: Vec<String> = self
            .nodes
            .values()
            .filter(|n| n.capability.usable())
            .filter(|n| {
                let d = &n.capability.declared;
                d.get("model_hash").and_then(|v| v.as_str()) == Some(model_hash)
            })
            .filter(|n| {
                wymagany_ctx == 0 || {
                    let limit = n
                        .capability
                        .declared
                        .get("max_ctx")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    limit == 0 || wymagany_ctx <= limit as u32
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
        out.sort(); // deterministycznie — losowanie robi warstwa wyżej
        out
    }

    pub fn assign(&mut self, job_id: &str, node_id: &str) -> bool {
        // RULES #2: wyłącznie node z POTWIERDZONĄ mocą.
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

    /// Przyjmuje wynik. NAPRAWA F3: sukces wymaga ważnego receiptu
    /// pod kluczem ZAREJESTROWANYM dla tego node'a (wariant C).
    ///
    /// Porażka (`ok=false`) receiptu nie wymaga — nie ma czego rozliczać.
    pub fn complete(&mut self, result: &JobResult) -> Result<(), SimonError> {
        let assigned_to = self
            .assigned
            .get(&result.job_id)
            .ok_or_else(|| SimonError::ReceiptMismatch("job nie jest przydzielony".into()))?;

        if assigned_to != &result.node_id {
            return Err(SimonError::ReceiptMismatch(
                "wynik od node'a, któremu nie przydzielono joba".into(),
            ));
        }

        if result.ok {
            let receipt = result
                .receipt
                .as_ref()
                .ok_or_else(|| SimonError::ReceiptMismatch("sukces bez receiptu".into()))?;

            let registered = self
                .nodes
                .get(&result.node_id)
                .ok_or(SimonError::UnregisteredKey)?;

            receipt.verify_for_node(&result.job_id, &result.node_id, &registered.pubkey)?;
        }

        self.assigned.remove(&result.job_id);
        Ok(())
    }

    /// Ewikcja node'a (RULES: ścieżka degradacji). Zwraca joby do ponownego przydziału.
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
}
