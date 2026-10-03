//! Weryfikacja receiptu SIMON — JEDNO ŹRÓDŁO PRAWDY, eksport do WASM.
//!
//! Ten crate świadomie NIE zawiera własnej kryptografii. Woła dokładnie te
//! same bramki co `simon --verify-receipt` w CLI i co rdzeń:
//!
//!   1. `Receipt::verify_self()`      — Ed25519 po odcisku CAŁEGO receiptu
//!                                      (nie po `sha256(output)`),
//!   2. `Receipt::zgodny_z_wyjsciem()` — domenowo rozdzielony, związany z
//!                                      `job_id` odcisk treści (`SIMON/OUTPUT/v1`),
//!   3. opcjonalnie zgodność `job_id` / `model_hash`.
//!
//! Dzięki temu UI (przeglądarka, bez serwera) pokazuje tylko werdykt, a nie
//! liczy hashy po swojemu — koniec rozjazdu `ui/src/lib/crypto.ts` vs
//! `simon-core`.

use serde::Serialize;
use simon_core::receipt::Receipt;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct Werdykt {
    parsuje_sie: bool,
    podpis_ok: bool,
    tresc_ok: Option<bool>,
    job_id_ok: Option<bool>,
    model_ok: Option<bool>,
    ok: bool,
    job_id: Option<String>,
    node_id: Option<String>,
    model_hash: Option<String>,
    runtime: Option<String>,
    signer: Option<String>,
    output_digest: Option<String>,
    powod: Option<String>,
}

impl Werdykt {
    fn odrzucony(powod: String) -> Self {
        Self {
            parsuje_sie: false,
            podpis_ok: false,
            tresc_ok: None,
            job_id_ok: None,
            model_ok: None,
            ok: false,
            job_id: None,
            node_id: None,
            model_hash: None,
            runtime: None,
            signer: None,
            output_digest: None,
            powod: Some(powod),
        }
    }
}

fn jako_json(w: &Werdykt) -> String {
    serde_json::to_string(w).unwrap_or_else(|_| {
        "{\"parsuje_sie\":false,\"podpis_ok\":false,\"ok\":false,\"powod\":\"serializacja\"}"
            .to_string()
    })
}

/// Weryfikuje receipt (JSON) wobec opcjonalnej treści odpowiedzi oraz
/// opcjonalnego oczekiwanego `job_id` / `model_hash`. Zwraca JSON z osobnymi
/// bramkami — UI wyłącznie prezentuje wynik.
///
/// * `receipt_json` — receipt w formacie SIMON (JSON),
/// * `output`       — treść odpowiedzi, którą rzekomo opisuje receipt
///                    (bez niej bramka treści jest `null` = nie sprawdzona),
/// * `oczekiwany_job_id` / `oczekiwany_model` — opcjonalne bramki zgodności.
#[wasm_bindgen]
pub fn weryfikuj(
    receipt_json: &str,
    output: Option<String>,
    oczekiwany_job_id: Option<String>,
    oczekiwany_model: Option<String>,
) -> String {
    let r: Receipt = match serde_json::from_str(receipt_json.trim()) {
        Ok(r) => r,
        Err(e) => return jako_json(&Werdykt::odrzucony(format!("nieparsowalny receipt: {e}"))),
    };

    let podpis_ok = r.verify_self().is_ok();
    let tresc_ok = output.as_deref().map(|o| r.zgodny_z_wyjsciem(o));
    let job_id_ok = oczekiwany_job_id.as_deref().map(|j| j == r.job_id);
    let model_ok = oczekiwany_model
        .as_deref()
        .map(|m| m == r.model_hash);

    let ok = podpis_ok
        && tresc_ok != Some(false)
        && job_id_ok != Some(false)
        && model_ok != Some(false);

    jako_json(&Werdykt {
        parsuje_sie: true,
        podpis_ok,
        tresc_ok,
        job_id_ok,
        model_ok,
        ok,
        job_id: Some(r.job_id.clone()),
        node_id: Some(r.node_id.clone()),
        model_hash: Some(r.model_hash.clone()),
        runtime: Some(r.runtime.clone()),
        signer: Some(r.signer.to_hex()),
        output_digest: Some(r.output_digest.clone()),
        powod: if ok {
            None
        } else if !podpis_ok {
            Some("podpis Ed25519 nie pasuje do odcisku receiptu".into())
        } else if tresc_ok == Some(false) {
            Some("odcisk treści (job_id + output) nie zgadza się z receiptem".into())
        } else {
            Some("nie wszystkie bramki zgodności przeszły".into())
        },
    })
}

/// Odcisk treści wyniku, liczący się z receiptem — do podglądu/diagnostyki.
/// Ten sam wzór co w `odcisk_wyjscia` rdzenia.
#[wasm_bindgen]
pub fn odcisk_wyjscia(job_id: &str, output: &str) -> Option<String> {
    simon_core::receipt::odcisk_wyjscia(job_id, output).ok()
}

/// Bramka M1: czy receipt wiąże DOKŁADNIE ten tokenizer, nonce, prompt i wyjście
/// (po tokenach). Tokeny podajemy jako JSON-owe tablice u32. Czysty CPU — bez
/// wag i bez GPU. To ta sama logika co `Receipt::zweryfikuj_m1` w rdzeniu.
#[wasm_bindgen]
pub fn weryfikuj_m1(
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
                "ok": false, "poziom": 0,
                "powod": format!("nieparsowalny receipt: {e}")
            })
            .to_string()
        }
    };
    let p: Vec<u32> = match serde_json::from_str(prompt_token_ids_json) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::json!({
                "ok": false, "poziom": r.poziom(),
                "powod": format!("prompt_token_ids_json: {e}")
            })
            .to_string()
        }
    };
    let o: Vec<u32> = match serde_json::from_str(output_token_ids_json) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::json!({
                "ok": false, "poziom": r.poziom(),
                "powod": format!("output_token_ids_json: {e}")
            })
            .to_string()
        }
    };
    match r.zweryfikuj_m1(tokenizer_hash, client_nonce, &p, &o) {
        Ok(()) => serde_json::json!({"ok": true, "poziom": r.poziom(), "powod": null}).to_string(),
        Err(e) => serde_json::json!({"ok": false, "poziom": r.poziom(), "powod": e.to_string()})
            .to_string(),
    }
}

#[cfg(test)]
mod testy {
    use super::weryfikuj;

    // Prawdziwy receipt z repo (nie wymyślony): przyklady/receipt.json +
    // przyklady/odpowiedz.txt. To jest ten sam plik, który dostaje sędzia.
    const RECEIPT: &str = include_str!("../../../przyklady/receipt.json");
    const OUTPUT: &str = include_str!("../../../przyklady/odpowiedz.txt");

    fn pole<'a>(json: &'a str, klucz: &str) -> serde_json::Value {
        serde_json::from_str::<serde_json::Value>(json)
            .expect("JSON")
            .get(klucz)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    }

    #[test]
    fn poprawny_receipt_przechodzi() {
        let w = weryfikuj(RECEIPT, Some(OUTPUT.to_string()), None, None);
        assert_eq!(pole(&w, "ok"), serde_json::json!(true), "{w}");
        assert_eq!(pole(&w, "podpis_ok"), serde_json::json!(true), "{w}");
        assert_eq!(pole(&w, "tresc_ok"), serde_json::json!(true), "{w}");
    }

    #[test]
    fn jedna_spacja_lamie_tresc_ale_nie_podpis() {
        let w = weryfikuj(RECEIPT, Some(format!("{OUTPUT} ")), None, None);
        assert_eq!(pole(&w, "ok"), serde_json::json!(false), "{w}");
        assert_eq!(pole(&w, "tresc_ok"), serde_json::json!(false), "{w}");
        // Podpis jest nad odciskiem całego receiptu, więc dodanie spacji do
        // TREŚCI (nie do receiptu) go nie łamie — i właśnie dlatego bramka
        // treści musi istnieć.
        assert_eq!(pole(&w, "podpis_ok"), serde_json::json!(true), "{w}");
    }

    #[test]
    fn nieparsowalny_receipt_jest_odrzucony() {
        let w = weryfikuj("{}", Some("cokolwiek".into()), None, None);
        assert_eq!(pole(&w, "ok"), serde_json::json!(false), "{w}");
        assert_eq!(pole(&w, "parsuje_sie"), serde_json::json!(false), "{w}");
    }

    #[test]
    fn m1_wiazanie_przechodzi_i_tamper_lamie() {
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
            output_digest: simon_core::receipt::odcisk_wyjscia("j", "x").unwrap(),
            prompt_tokens: 3,
            completion_tokens: 3,
            started_at_us: 0,
            finished_at_us: 1,
            signer: k.public(),
            signature: None,
            wiazania: None,
        };
        let r = base
            .z_wiazaniem_m1(
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

        // poprawnie
        let ok = super::weryfikuj_m1(&rj, "TH", "NONCE", "[1,2,3]", "[7,8,9]");
        assert_eq!(pole(&ok, "ok"), serde_json::json!(true), "{ok}");
        assert_eq!(pole(&ok, "poziom"), serde_json::json!(1), "{ok}");
        // podmieniony output token
        let bad = super::weryfikuj_m1(&rj, "TH", "NONCE", "[1,2,3]", "[7,8,999]");
        assert_eq!(pole(&bad, "ok"), serde_json::json!(false), "{bad}");
        // zły nonce
        let bad2 = super::weryfikuj_m1(&rj, "TH", "INNY", "[1,2,3]", "[7,8,9]");
        assert_eq!(pole(&bad2, "ok"), serde_json::json!(false), "{bad2}");
    }
}
