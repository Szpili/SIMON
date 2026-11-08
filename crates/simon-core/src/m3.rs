//! M3 — execution verification via **containment in top-k**.
//!
//! Empirical basis (brain: `simon-e0-fp-divergence`, `simon-m3-topk-kalibracja`):
//! - Exact-match CPU<->GPU at temp=0 is invalid due to FP divergence (~3 tokens).
//! - FP divergence is almost always a top-1 vs top-2 dispute (logprob margin 0.07–0.17).
//! - When verifier follows node trajectory (prefix-forcing on tokens), node's token
//!   falls in top-10 in 150/150 steps, and in top-2 in 100% of steps.

use serde::{Deserialize, Serialize};

use crate::{content_digest, SimonError};

/// Domain separator for audit digest.
pub const AUDIT_DOMAIN: &str = "SIMON/AUDIT/v1";
pub const DOMENA_AUDYTU: &str = AUDIT_DOMAIN;

/// One verified step: node token vs verifier distribution at this step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogprobStep {
    #[serde(rename = "index", alias = "indeks")]
    pub index: u32,
    /// Token selected by the node (from its `output_token_ids`).
    pub node_token: u32,
    /// Verifier top-k: `(token_id, logprob)`, sorted descending by logprob.
    pub topk: Vec<(u32, f64)>,
}

pub type Krok = LogprobStep;

impl LogprobStep {
    pub fn indeks(&self) -> u32 {
        self.index
    }
}

/// Acceptance policy. Default values derived from M3 empirical calibration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct M3Policy {
    /// How many top verifier tokens are considered compliant.
    pub k: usize,
    /// Maximum margin `logprob(top1) - logprob(node)` when node is in top-k.
    pub max_margin: f64,
    /// Allowed fraction of suspicious steps (outside top-k or exceeding max_margin).
    #[serde(rename = "max_outside", alias = "max_poza")]
    pub max_outside: f64,
}

pub type Polityka = M3Policy;

impl Default for M3Policy {
    fn default() -> Self {
        Self {
            k: 2,
            max_margin: 0.25,
            max_outside: 0.02,
        }
    }
}

/// Audit verdict. All fields are deterministic and suitable for logging.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct M3Verdict {
    #[serde(rename = "steps", alias = "krokow")]
    pub steps: usize,
    #[serde(rename = "in_topk", alias = "w_topk")]
    pub in_topk: usize,
    #[serde(rename = "outside_topk", alias = "poza_topk")]
    pub outside_topk: usize,
    /// Worst observed rank within top-k (0 = outside top-k).
    #[serde(rename = "worst_rank", alias = "najgorszy_rank")]
    pub worst_rank: usize,
    /// Maximum logprob margin relative to top-1.
    #[serde(rename = "worst_margin", alias = "najgorszy_margin")]
    pub worst_margin: f64,
    /// Mean NLL (negative logprob) of node token under verifier distribution.
    #[serde(rename = "mean_nll", alias = "sredni_nll")]
    pub mean_nll: f64,
    /// Count of steps where node token is completely absent from top-k.
    #[serde(rename = "not_in_list", alias = "poza_lista")]
    pub not_in_list: usize,
    pub ok: bool,
    #[serde(rename = "reason", alias = "powod")]
    pub reason: Option<String>,
}

pub type Werdykt = M3Verdict;

impl M3Verdict {
    pub fn krokow(&self) -> usize {
        self.steps
    }
    pub fn w_topk(&self) -> usize {
        self.in_topk
    }
    pub fn poza_topk(&self) -> usize {
        self.outside_topk
    }
    pub fn najgorszy_rank(&self) -> usize {
        self.worst_rank
    }
    pub fn najgorszy_margin(&self) -> f64 {
        self.worst_margin
    }
    pub fn sredni_nll(&self) -> f64 {
        self.mean_nll
    }
    pub fn poza_lista(&self) -> usize {
        self.not_in_list
    }
    pub fn powod(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

/// Execution audit input matching verifier JSON structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionAudit {
    #[serde(rename = "steps", alias = "kroki")]
    pub steps: Vec<LogprobStep>,
}

pub type Audyt = ExecutionAudit;

/// Evaluates steps against the given policy.
pub fn evaluate(steps: &[LogprobStep], p: &M3Policy) -> M3Verdict {
    let mut in_topk = 0usize;
    let mut outside = 0usize;
    let mut worst_rank = 0usize;
    let mut worst_margin = 0.0f64;
    let mut sum_nll = 0.0f64;
    let mut count_nll = 0usize;
    let mut not_in_list = 0usize;
    let mut reason: Option<String> = None;

    for (i, step) in steps.iter().enumerate() {
        if step.topk.iter().any(|(_, lp)| !lp.is_finite()) {
            outside += 1;
            not_in_list += 1;
            if reason.is_none() {
                reason = Some(format!(
                    "step {i} (idx {}): non-finite logprob in top-k",
                    step.index
                ));
            }
            continue;
        }

        let mut sorted = step.topk.clone();
        sorted.sort_by(|a, b| b.1.total_cmp(&a.1));

        let top1_lp = sorted.first().map(|x| x.1);
        let match_found = sorted
            .iter()
            .enumerate()
            .find(|(_, (id, _))| *id == step.node_token);

        match match_found {
            Some((rank0, (_, lp))) => {
                let rank = rank0 + 1;
                sum_nll += -lp;
                count_nll += 1;
                let margin = match top1_lp {
                    Some(t1) => (t1 - *lp).max(0.0),
                    None => 0.0,
                };
                if rank > worst_rank {
                    worst_rank = rank;
                }
                if margin > worst_margin {
                    worst_margin = margin;
                }
                if rank <= p.k && margin <= p.max_margin {
                    in_topk += 1;
                } else {
                    outside += 1;
                    if reason.is_none() {
                        reason = Some(format!(
                            "step {i} (idx {}): rank {rank} (k={}), margin {margin:.4} (max {:.4})",
                            step.index, p.k, p.max_margin
                        ));
                    }
                }
            }
            None => {
                outside += 1;
                not_in_list += 1;
                if reason.is_none() {
                    reason = Some(format!(
                        "step {i} (idx {}): token {} OUTSIDE top-{}",
                        step.index, step.node_token, p.k
                    ));
                }
            }
        }
    }

    let fraction = if steps.is_empty() {
        1.0
    } else {
        outside as f64 / steps.len() as f64
    };
    let ok = !steps.is_empty() && fraction <= p.max_outside;
    if ok {
        reason = None;
    }

    M3Verdict {
        steps: steps.len(),
        in_topk,
        outside_topk: outside,
        worst_rank,
        worst_margin,
        mean_nll: if count_nll > 0 { sum_nll / count_nll as f64 } else { 0.0 },
        not_in_list,
        ok,
        reason,
    }
}

/// Canonical audit digest over integers only (indices and token IDs).
pub fn audit_digest(steps: &[LogprobStep]) -> Result<String, SimonError> {
    let simplified: Vec<(u32, u32, Vec<u32>)> = steps
        .iter()
        .map(|k| {
            (
                k.index,
                k.node_token,
                k.topk.iter().map(|(id, _)| *id).collect(),
            )
        })
        .collect();
    content_digest(&serde_json::json!({
        "domena": AUDIT_DOMAIN,
        "kroki": simplified,
    }))
}

pub use audit_digest as odcisk_audytu;

/// Parses audit JSON and evaluates against policy.
pub fn evaluate_json(json: &str, p: &M3Policy) -> Result<M3Verdict, SimonError> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    if v.get("simulated").is_some_and(|s| s != &serde_json::Value::Bool(false)) {
        return Err(SimonError::ReceiptMismatch(
            "simulated audit (verifier_loop --dry-run) is not evidence".into(),
        ));
    }
    let a: ExecutionAudit = serde_json::from_value(v)?;
    Ok(evaluate(&a.steps, p))
}

/// Decision from M3 audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Pass,
    SoftFail,
    HardFail,
    InsufficientEvidence,
    #[serde(alias = "brak_podstaw")]
    InsufficientBasis,
}

pub type Decyzja = Decision;

/// Decision thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct M3Thresholds {
    #[serde(rename = "hard_outside", alias = "twardy_poza")]
    pub hard_outside: f64,
    #[serde(rename = "min_steps", alias = "min_krokow")]
    pub min_steps: usize,
}

pub type Progi = M3Thresholds;

impl Default for M3Thresholds {
    fn default() -> Self {
        Self { hard_outside: 0.05, min_steps: 8 }
    }
}

/// Converts M3 verdict and receipt level into a decision.
pub fn decide(verdict: &M3Verdict, receipt_level: u8, thresholds: &M3Thresholds) -> Decision {
    if receipt_level < 1 || verdict.steps == 0 {
        return Decision::InsufficientBasis;
    }
    if verdict.steps < thresholds.min_steps {
        return Decision::InsufficientEvidence;
    }
    if verdict.ok {
        return Decision::Pass;
    }
    Decision::SoftFail
}

pub use decide as decyzja;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_audit_rejected() {
        let p = M3Policy::default();
        let valid = r#"{"steps":[{"index":0,"node_token":5,"topk":[[5,-0.01],[6,-2.0]]}]}"#;
        assert!(evaluate_json(valid, &p).is_ok());
        let simulated = r#"{"simulated":true,"steps":[{"index":0,"node_token":5,"topk":[[5,-0.01],[6,-2.0]]}]}"#;
        assert!(evaluate_json(simulated, &p).is_err());
        for bad in ["true", r#""true""#, "1", "null"] {
            let j = format!(r#"{{"simulated":{bad},"steps":[{{"index":0,"node_token":5,"topk":[[5,-0.01],[6,-2.0]]}}]}}"#);
            let e = evaluate_json(&j, &p).expect_err("simulated != false must reject");
            assert!(e.to_string().contains("simulated"));
        }
        let explicitly_not = r#"{"simulated":false,"steps":[{"index":0,"node_token":5,"topk":[[5,-0.01],[6,-2.0]]}]}"#;
        assert!(evaluate_json(explicitly_not, &p).is_ok());
    }

    fn topk(pairs: &[(u32, f64)]) -> Vec<(u32, f64)> {
        pairs.to_vec()
    }

    #[test]
    fn honest_divergence_top1_top2_passes() {
        let steps = vec![
            LogprobStep { index: 0, node_token: 528, topk: topk(&[(528, -1.20), (28742, -1.37)]) },
            LogprobStep { index: 1, node_token: 302, topk: topk(&[(28725, -0.90), (302, -0.96)]) },
            LogprobStep { index: 2, node_token: 28783, topk: topk(&[(28783, -2.10), (28774, -2.17)]) },
        ];
        let w = evaluate(&steps, &M3Policy::default());
        assert!(w.ok, "{w:?}");
        assert_eq!(w.steps, 3);
        assert_eq!(w.in_topk, 3);
        assert_eq!(w.outside_topk, 0);
        assert_eq!(w.worst_rank, 2);
        assert!(w.worst_margin <= 0.25 + 1e-9);
    }

    #[test]
    fn token_outside_topk_rejected() {
        let steps = vec![
            LogprobStep { index: 0, node_token: 528, topk: topk(&[(528, -1.0)]) },
            LogprobStep { index: 1, node_token: 999999, topk: topk(&[(28725, -0.9), (302, -0.96)]) },
        ];
        let w = evaluate(&steps, &M3Policy::default());
        assert!(!w.ok, "{w:?}");
        assert_eq!(w.outside_topk, 1);
        assert!(w.reason.unwrap().contains("OUTSIDE top-2"));
    }

    #[test]
    fn large_margin_is_suspicious() {
        let steps = vec![LogprobStep {
            index: 0,
            node_token: 302,
            topk: topk(&[(28725, -0.1), (302, -3.0)]),
        }];
        let w = evaluate(&steps, &M3Policy::default());
        assert!(!w.ok, "{w:?}");
        assert!(w.worst_margin > 2.0);
    }

    #[test]
    fn empty_audit_fails() {
        let w = evaluate(&[], &M3Policy::default());
        assert!(!w.ok);
        assert_eq!(w.steps, 0);
    }

    #[test]
    fn decision_blocks_slash_without_level_1_or_empty_audit() {
        let w = evaluate(&[], &M3Policy::default());
        assert_eq!(
            decide(&w, 0, &M3Thresholds::default()),
            Decision::InsufficientBasis
        );
        assert_eq!(
            decide(&w, 1, &M3Thresholds::default()),
            Decision::InsufficientBasis
        );
    }
}
