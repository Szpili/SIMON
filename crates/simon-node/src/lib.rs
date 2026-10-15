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

/// Topic dla wyników wykonania zadania przez node.
pub const TOPIC_JOB_RESULT: &str = "simon/v1/job-result";

/// Rejestr node'ów + jobów. Deterministyczny, bez sieci — testowalny bez I/O.
#[derive(Default)]
pub struct Registry {
    pub nodes: HashMap<String, RegisterMsg>,
    pub states: HashMap<String, NodeState>,
    /// Joby w toku: job_id -> node_id, do którego przydzielono.
    pub assigned: HashMap<String, String>,
    pub timeout_s: Option<u64>,
    pub last_seen: HashMap<String, u64>,
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

    /// Oznacza węzeł jako zdegradowany (np. przy błędzie wykonania lub pustym wyniku).
    pub fn mark_degraded(&mut self, node_id: &str) -> Result<(), SimonError> {
        if !self.nodes.contains_key(node_id) {
            return Err(SimonError::UnregisteredKey);
        }
        self.states.insert(node_id.to_string(), NodeState::Degraded);
        Ok(())
    }

    /// Przywraca stan Healthy dla zarejestrowanego węzła.
    pub fn heartbeat(&mut self, node_id: &str) -> Result<(), SimonError> {
        if !self.nodes.contains_key(node_id) {
            return Err(SimonError::UnregisteredKey);
        }
        self.states.insert(node_id.to_string(), NodeState::Healthy);
        Ok(())
    }

    /// Rejestruje heartbeat węzła wraz ze stemplem czasowym.
    pub fn record_heartbeat(&mut self, node_id: &str, teraz_s: u64) -> Result<(), SimonError> {
        if !self.nodes.contains_key(node_id) {
            return Err(SimonError::UnregisteredKey);
        }
        self.states.insert(node_id.to_string(), NodeState::Healthy);
        self.last_seen.insert(node_id.to_string(), teraz_s);
        Ok(())
    }

    /// Liczba aktualnie przypisanych jobów.
    pub fn active_assignments(&self) -> usize {
        self.assigned.len()
    }

    /// Zdejmuje przypisanie joba po identyfikatorze bez weryfikacji receiptu.
    pub fn complete_job(&mut self, job_id: &str) -> Option<String> {
        self.assigned.remove(job_id)
    }

    /// Deterministyczny wybór kandydata za pomocą VRF seed.
    ///
    /// 1. Filtruje kandydatów po modelu i limicie kontekstu (oraz timeout_s jeśli ustawiony).
    /// 2. Liczy deterministyczny seed: `content_digest(&("SIMON/VRF/v1", &(model_hash, job_id)))`.
    /// 3. Wybiera kandydata o najmniejszym leksykograficznie hashu wyboru:
    ///    `content_digest(&("SIMON/VRF-PICK/v1", &(&vrf_seed, cand)))`.
    pub fn wybierz_kandydata_vrf(
        &self,
        model_hash: &str,
        wymagany_ctx: u32,
        job_id: &str,
        teraz_s: u64,
    ) -> Option<(String, String)> {
        let kandydaci = self.kandydaci_dla_modelu(model_hash, wymagany_ctx);
        let kandydaci: Vec<String> = if let Some(timeout) = self.timeout_s {
            kandydaci
                .into_iter()
                .filter(|cand| {
                    if let Some(last) = self.last_seen.get(cand) {
                        teraz_s.saturating_sub(*last) <= timeout
                    } else {
                        true
                    }
                })
                .collect()
        } else {
            kandydaci
        };

        if kandydaci.is_empty() {
            return None;
        }

        let vrf_seed = simon_core::content_digest(&("SIMON/VRF/v1", &(model_hash, job_id))).ok()?;

        let mut najlepszy: Option<(String, String)> = None;
        for cand in kandydaci {
            let pick_digest =
                simon_core::content_digest(&("SIMON/VRF-PICK/v1", &(&vrf_seed, &cand))).ok()?;
            match &najlepszy {
                None => najlepszy = Some((cand, pick_digest)),
                Some((_, best_digest)) => {
                    if &pick_digest < best_digest {
                        najlepszy = Some((cand, pick_digest));
                    }
                }
            }
        }

        najlepszy.map(|(node_id, _)| (node_id, vrf_seed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simon_core::Keypair;

    fn test_node(id: &str, model: &str, max_ctx: u64) -> RegisterMsg {
        let key = Keypair::generate();
        let cap = Capability {
            declared: serde_json::json!({
                "model_hash": model,
                "max_ctx": max_ctx,
            }),
            attested: Some(serde_json::json!({
                "model_hash": model,
                "max_ctx": max_ctx,
            })),
        };
        RegisterMsg::new(id, key.public(), cap)
    }

    #[test]
    fn test_mark_degraded_and_heartbeat_recovery() {
        let mut reg = Registry::new();
        let node = test_node("node-1", "model-a", 2048);
        assert!(reg.register(node));

        let kandydaci = reg.kandydaci_dla_modelu("model-a", 1024);
        assert_eq!(kandydaci, vec!["node-1".to_string()]);

        assert!(reg.mark_degraded("node-1").is_ok());
        assert_eq!(reg.states.get("node-1"), Some(&NodeState::Degraded));
        assert!(reg.kandydaci_dla_modelu("model-a", 1024).is_empty());
        assert!(!reg.assign("job-1", "node-1"));

        // Heartbeat przywraca węzeł
        assert!(reg.heartbeat("node-1").is_ok());
        assert_eq!(reg.states.get("node-1"), Some(&NodeState::Healthy));
        assert_eq!(reg.kandydaci_dla_modelu("model-a", 1024), vec!["node-1".to_string()]);
        assert!(reg.assign("job-1", "node-1"));
        assert_eq!(reg.active_assignments(), 1);
    }

    #[test]
    fn test_vrf_deterministic_selection() {
        let mut reg = Registry::new();
        for i in 1..=4 {
            let id = format!("node-{i}");
            reg.register(test_node(&id, "model-x", 4096));
        }

        let job_id = "job-42";
        let res1 = reg.wybierz_kandydata_vrf("model-x", 2048, job_id, 1000);
        let res2 = reg.wybierz_kandydata_vrf("model-x", 2048, job_id, 1000);
        let res3 = reg.wybierz_kandydata_vrf("model-x", 2048, job_id, 1000);

        assert!(res1.is_some());
        assert_eq!(res1, res2);
        assert_eq!(res2, res3);

        let (node, seed) = res1.unwrap();
        assert!(!node.is_empty());
        assert!(!seed.is_empty());
    }

    #[test]
    fn test_vrf_distribution() {
        let mut reg = Registry::new();
        for i in 1..=5 {
            let id = format!("node-{i}");
            reg.register(test_node(&id, "model-dist", 4096));
        }

        let mut wybrane_wezly = std::collections::HashSet::new();
        for j in 0..50 {
            let job_id = format!("job-{j}");
            if let Some((node, _)) = reg.wybierz_kandydata_vrf("model-dist", 1024, &job_id, 1000) {
                wybrane_wezly.insert(node);
            }
        }
        // VRF powinien rozdzielić 50 różnych jobów na więcej niż 1 węzeł (nie zawsze pierwszy leksykograficznie)
        assert!(wybrane_wezly.len() > 1, "VRF musi rozdzielać zadania na różne węzły");
    }

    #[test]
    fn test_vrf_stale_node_timeout() {
        let mut reg = Registry::new();
        reg.timeout_s = Some(60);
        let n1 = test_node("node-fresh", "model-t", 2048);
        let n2 = test_node("node-stale", "model-t", 2048);
        reg.register(n1);
        reg.register(n2);

        reg.record_heartbeat("node-fresh", 1000).unwrap();
        reg.record_heartbeat("node-stale", 900).unwrap();

        // Przy teraz_s = 1010: fresh ma delta 10s (<=60), stale ma delta 110s (>60)
        let wybrany = reg.wybierz_kandydata_vrf("model-t", 1024, "job-t", 1010);
        assert!(wybrany.is_some());
        assert_eq!(wybrany.unwrap().0, "node-fresh");
    }
}
