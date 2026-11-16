//! Receipt verification for SIMON — SINGLE SOURCE OF TRUTH, exported to WASM.
//!
//! This crate deliberately does NOT contain its own cryptography. It calls
//! the exact same gates as `simon --verify-receipt` in the CLI and core:
//!
//!   1. `Receipt::verify_self()`      — Ed25519 over the digest of the ENTIRE receipt
//!      (not over `sha256(output)`),
//!   2. `Receipt::matches_output()`  — domain-separated, bound to `job_id`
//!      content digest (`SIMON/OUTPUT/v1`),
//!   3. optionally checks `job_id` / `model_hash` match.
//!
//! This ensures the UI (browser, serverless) only displays the verdict and
//! does not compute hashes independently — eliminating drift between
//! `ui/src/lib/crypto.ts` and `simon-core`.

#![allow(clippy::doc_overindented_list_items)]

use serde::Serialize;
use simon_core::receipt::Receipt;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
pub struct Verdict {
    pub parses: bool,
    pub signature_ok: bool,
    pub content_ok: Option<bool>,
    pub job_id_ok: Option<bool>,
    pub model_ok: Option<bool>,
    pub ok: bool,
    pub job_id: Option<String>,
    pub node_id: Option<String>,
    pub model_hash: Option<String>,
    pub runtime: Option<String>,
    pub signer: Option<String>,
    pub output_digest: Option<String>,
    pub reason: Option<String>,

    // Backwards-compatibility aliases for UI
    #[serde(rename = "parsuje_sie")]
    pub parsuje_sie: bool,
    #[serde(rename = "podpis_ok")]
    pub podpis_ok: bool,
    #[serde(rename = "tresc_ok")]
    pub tresc_ok: Option<bool>,
    #[serde(rename = "powod")]
    pub powod: Option<String>,
}

impl Verdict {
    fn rejected(reason: String) -> Self {
        Self {
            parses: false,
            signature_ok: false,
            content_ok: None,
            job_id_ok: None,
            model_ok: None,
            ok: false,
            job_id: None,
            node_id: None,
            model_hash: None,
            runtime: None,
            signer: None,
            output_digest: None,
            reason: Some(reason.clone()),
            parsuje_sie: false,
            podpis_ok: false,
            tresc_ok: None,
            powod: Some(reason),
        }
    }
}

fn as_json(v: &Verdict) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| {
        "{\"parses\":false,\"signature_ok\":false,\"ok\":false,\"reason\":\"serialization\",\"parsuje_sie\":false,\"podpis_ok\":false,\"powod\":\"serializacja\"}"
            .to_string()
    })
}

/// Verifies a receipt (JSON) against optional response output and
/// optional expected `job_id` / `model_hash`. Returns JSON with individual gates.
#[wasm_bindgen]
pub fn verify_receipt(
    receipt_json: &str,
    output: Option<String>,
    expected_job_id: Option<String>,
    expected_model: Option<String>,
) -> String {
    let r: Receipt = match serde_json::from_str(receipt_json.trim()) {
        Ok(r) => r,
        Err(e) => return as_json(&Verdict::rejected(format!("unparseable receipt: {e}"))),
    };

    let signature_ok = r.verify_self().is_ok();
    let content_ok = output.as_deref().map(|o| r.matches_output(o));
    let job_id_ok = expected_job_id.as_deref().map(|j| j == r.job_id);
    let model_ok = expected_model.as_deref().map(|m| m == r.model_hash);

    let ok = signature_ok
        && content_ok != Some(false)
        && job_id_ok != Some(false)
        && model_ok != Some(false);

    let reason: Option<String> = if ok {
        None
    } else if !signature_ok {
        Some("Ed25519 signature does not match the receipt digest".into())
    } else if content_ok == Some(false) {
        Some("output digest (job_id + output) does not match the receipt".into())
    } else {
        Some("not all checks passed".into())
    };

    as_json(&Verdict {
        parses: true,
        signature_ok,
        content_ok,
        job_id_ok,
        model_ok,
        ok,
        job_id: Some(r.job_id.clone()),
        node_id: Some(r.node_id.clone()),
        model_hash: Some(r.model_hash.clone()),
        runtime: Some(r.runtime.clone()),
        signer: Some(r.signer.to_hex()),
        output_digest: Some(r.output_digest.clone()),
        reason: reason.clone(),
        parsuje_sie: true,
        podpis_ok: signature_ok,
        tresc_ok: content_ok,
        powod: reason,
    })
}

/// Canonical output digest matching receipt expectation.
#[wasm_bindgen]
pub fn output_digest(job_id: &str, output: &str) -> Option<String> {
    simon_core::receipt::output_digest_for(job_id, output).ok()
}

/// Gate M1: whether the receipt binds EXACTLY this tokenizer, nonce, prompt, and output tokens.
#[wasm_bindgen]
pub fn verify_m1(
    receipt_json: &str,
    tokenizer_hash: &str,
    client_nonce: &str,
    prompt_token_ids_json: &str,
    output_token_ids_json: &str,
) -> String {
    let r: Receipt = match serde_json::from_str(receipt_json.trim()) {
        Ok(r) => r,
        Err(e) => {
            return serde_json::json!({
                "ok": false, "level": 0,
                "reason": format!("unparseable receipt: {e}"),
            })
            .to_string()
        }
    };
    let p: Vec<u32> = match serde_json::from_str(prompt_token_ids_json) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::json!({
                "ok": false, "level": r.level(),
                "reason": format!("prompt_token_ids_json: {e}"),
            })
            .to_string()
        }
    };
    let o: Vec<u32> = match serde_json::from_str(output_token_ids_json) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::json!({
                "ok": false, "level": r.level(),
                "reason": format!("output_token_ids_json: {e}"),
            })
            .to_string()
        }
    };
    match r.verify_m1(tokenizer_hash, client_nonce, &p, &o) {
        Ok(()) => serde_json::json!({
            "ok": true,
            "level": r.level(),
            "reason": null,
        })
        .to_string(),
        Err(e) => serde_json::json!({
            "ok": false,
            "level": r.level(),
            "reason": e.to_string(),
        })
        .to_string(),
    }
}

/// Gate M3: evaluates execution audit (containment in top-k + margin).
#[wasm_bindgen]
pub fn evaluate_m3(audit_json: &str, k: u32, max_margin: f64, max_outside: f64) -> String {
    let policy = simon_core::m3::M3Policy {
        k: k as usize,
        max_margin,
        max_outside,
    };
    match simon_core::m3::evaluate_json(audit_json, &policy) {
        Ok(w) => serde_json::to_string(&w).unwrap_or_else(|_| "{\"ok\":false}".into()),
        Err(e) => {
            serde_json::json!({"ok": false, "reason": format!("invalid audit: {e}"), "powod": format!("invalid audit: {e}")}).to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate_m3, verify_m1, verify_receipt};

    const RECEIPT: &str = include_str!("../../../przyklady/receipt.json");
    const OUTPUT: &str = include_str!("../../../przyklady/odpowiedz.txt");

    fn get_field(json: &str, key: &str) -> serde_json::Value {
        serde_json::from_str::<serde_json::Value>(json)
            .expect("JSON")
            .get(key)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    }

    #[test]
    fn valid_receipt_passes() {
        let w = verify_receipt(RECEIPT, Some(OUTPUT.to_string()), None, None);
        assert_eq!(get_field(&w, "ok"), serde_json::json!(true), "{w}");
        assert_eq!(get_field(&w, "signature_ok"), serde_json::json!(true), "{w}");
        assert_eq!(get_field(&w, "content_ok"), serde_json::json!(true), "{w}");
        assert_eq!(get_field(&w, "podpis_ok"), serde_json::json!(true), "{w}");
        assert_eq!(get_field(&w, "tresc_ok"), serde_json::json!(true), "{w}");
    }

    #[test]
    fn single_space_breaks_content_not_signature() {
        let w = verify_receipt(RECEIPT, Some(format!("{OUTPUT} ")), None, None);
        assert_eq!(get_field(&w, "ok"), serde_json::json!(false), "{w}");
        assert_eq!(get_field(&w, "content_ok"), serde_json::json!(false), "{w}");
        assert_eq!(get_field(&w, "signature_ok"), serde_json::json!(true), "{w}");
    }

    #[test]
    fn unparseable_receipt_is_rejected() {
        let w = verify_receipt("{}", Some("anything".into()), None, None);
        assert_eq!(get_field(&w, "ok"), serde_json::json!(false), "{w}");
        assert_eq!(get_field(&w, "parses"), serde_json::json!(false), "{w}");
        assert_eq!(get_field(&w, "parsuje_sie"), serde_json::json!(false), "{w}");
    }

    #[test]
    fn m1_binding_passes_and_tamper_breaks() {
        use simon_core::crypto::Keypair;
        use simon_core::receipt::{Precision, Receipt};

        let k = Keypair::from_seed(&[3u8; 32]);
        let prompt = [1u32, 2, 3];
        let out = [7u32, 8, 9];
        let base = Receipt {
            job_id: "j".into(),
            node_id: "n".into(),
            model_hash: "m".into(),
            runtime: "llama.cpp/0.3".into(),
            precision: Precision::Fp16,
            activation_hash: "toploc:NIE_POLICZONY".into(),
            output_digest: simon_core::receipt::output_digest_for("j", "x").unwrap(),
            prompt_tokens: 3,
            completion_tokens: 3,
            started_at_us: 0,
            finished_at_us: 1,
            signer: k.public(),
            signature: None,
            bindings: None,
        };
        let r = base
            .clone()
            .with_m1_bindings(
                "TH".into(),
                "NONCE".into(),
                &prompt,
                &out,
                "SP".into(),
                7,
                "llama.cpp/Q4_K_M/cpu".into(),
            )
            .unwrap()
            .sign(&k)
            .unwrap();
        let rj = serde_json::to_string(&r).unwrap();

        // Valid
        let ok = verify_m1(&rj, "TH", "NONCE", "[1,2,3]", "[7,8,9]");
        assert_eq!(get_field(&ok, "ok"), serde_json::json!(true), "{ok}");
        assert_eq!(get_field(&ok, "level"), serde_json::json!(1), "{ok}");
        // Tampered output token
        let bad = verify_m1(&rj, "TH", "NONCE", "[1,2,3]", "[7,8,999]");
        assert_eq!(get_field(&bad, "ok"), serde_json::json!(false), "{bad}");
        // Wrong nonce
        let bad2 = verify_m1(&rj, "TH", "OTHER", "[1,2,3]", "[7,8,9]");
        assert_eq!(get_field(&bad2, "ok"), serde_json::json!(false), "{bad2}");
        // Unsigned receipt
        let rj_unsigned = serde_json::to_string(&base.with_m1_bindings(
            "TH".into(),
            "NONCE".into(),
            &prompt,
            &out,
            "SP".into(),
            7,
            "llama.cpp/Q4_K_M/cpu".into(),
        ).unwrap()).unwrap();
        let bad_unsigned = verify_m1(&rj_unsigned, "TH", "NONCE", "[1,2,3]", "[7,8,9]");
        assert_eq!(get_field(&bad_unsigned, "ok"), serde_json::json!(false), "{bad_unsigned}");
    }

    #[test]
    fn m3_honest_audit_passes_and_cheater_fails() {
        // Honest: node in top-2, margin <= 0.25
        let good = r#"{"steps":[
            {"index":0,"node_token":528,"topk":[[528,-1.20],[28742,-1.37]]},
            {"index":1,"node_token":302,"topk":[[28725,-0.90],[302,-0.96]]}
        ]}"#;
        let w = evaluate_m3(good, 2, 0.25, 0.02);
        assert_eq!(get_field(&w, "ok"), serde_json::json!(true), "{w}");
        assert_eq!(get_field(&w, "in_topk"), serde_json::json!(2), "{w}");

        // Cheater: one token outside top-k
        let bad = r#"{"steps":[
            {"index":0,"node_token":528,"topk":[[528,-1.20]]},
            {"index":1,"node_token":999999,"topk":[[28725,-0.90],[302,-0.96]]}
        ]}"#;
        let w2 = evaluate_m3(bad, 2, 0.25, 0.02);
        assert_eq!(get_field(&w2, "ok"), serde_json::json!(false), "{w2}");
        assert_eq!(get_field(&w2, "outside_topk"), serde_json::json!(1), "{w2}");
    }
}
