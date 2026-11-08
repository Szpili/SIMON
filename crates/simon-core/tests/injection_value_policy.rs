//! Testy na „10 sposobów złamania granicy w 10 minut" (ocena hackerska 2026-10-05).
//! Sedno: sama nazwa akcji to za mało — liczą się **wartości** i ich **pochodzenie**.
//! `waliduj` sprawdza kształt/zakres wartości; `przeplyw_ok` sprawdza provenance.
//! Decyzja = koniunkcja: (akcja+wartości OK) ORAZ (provenance → sink dozwolone).

use serde_json::json;
use simon_core::cap::{validate, ActionDef, ActionParam, CapabilityCeiling, ParamConstraint};
use simon_core::przeplyw::{przeplyw_ok, Sink, Zaufanie};

fn sufit() -> CapabilityCeiling {
    let mut actions = std::collections::BTreeMap::new();
    actions.insert(
        "send_email".into(),
        ActionDef {
            name: "send_email".into(),
            params: vec![
                ActionParam {
                    name: "to".into(),
                    required: true,
                    // Allowlist adresatów — sama nazwa akcji NIE wystarcza.
                    constraint: ParamConstraint::Enum {
                        values: vec!["boss@example.com".into(), "team@example.com".into()],
                    },
                },
                ActionParam {
                    name: "subject".into(),
                    required: true,
                    constraint: ParamConstraint::Text { max_len: 100 },
                },
            ],
        },
    );
    CapabilityCeiling { actions }
}

/// Pełna decyzja dla akcji: kształt wartości (I2) ORAZ provenance do sinku (I4).
fn czy_wolno(
    s: &CapabilityCeiling,
    akcja: &str,
    wartosci: &serde_json::Value,
    zrodlo: Zaufanie,
    deklasacje: &[(Zaufanie, Sink)],
) -> bool {
    if validate(s, akcja, wartosci).is_err() {
        return false;
    }
    przeplyw_ok(zrodlo, Sink::ActionArgument, deklasacje)
}

#[test]
fn value_allowlist_odrzuca_nieuprawnionego_adresata() {
    // #1/#3 z listy: akcja dozwolona, ale WARTOŚĆ zła (format poprawny).
    let s = sufit();
    let w = json!({"to": "attacker@evil.com", "subject": "hi"});
    assert!(!czy_wolno(&s, "send_email", &w, Zaufanie::Operator, &[]));
    // a dozwolony adresat przechodzi (provenance = operator)
    let ok = json!({"to": "boss@example.com", "subject": "hi"});
    assert!(czy_wolno(&s, "send_email", &ok, Zaufanie::Operator, &[]));
}

#[test]
fn poprawna_wartosc_ale_z_niezaufanego_zrodla_jest_odrzucona() {
    // #1/#4: wartość przechodzi allowlistę, ale pochodzi z treści niezaufanej
    // (RAG/peer/tool/prompt-injection) → do argumentu akcji NIE wolno bez deklasacji.
    let s = sufit();
    let w = json!({"to": "boss@example.com", "subject": "summary"});
    for zrodlo in [
        Zaufanie::RetrievedText,
        Zaufanie::PeerText,
        Zaufanie::ToolText,
        Zaufanie::ModelDerived,
    ] {
        assert!(
            !czy_wolno(&s, "send_email", &w, zrodlo, &[]),
            "{zrodlo:?} nie może sterować adresatem bez deklasacji"
        );
        // jawna deklasacja operatora otwiera TYLKO ten przepływ
        assert!(czy_wolno(&s, "send_email", &w, zrodlo, &[(zrodlo, Sink::ActionArgument)]));
    }
}

#[test]
fn tool_result_injection_nie_steruje_akcja() {
    // #2: wstrzyknięcie w wyniku narzędzia. ToolText → ActionArgument zawsze deny.
    assert!(!przeplyw_ok(Zaufanie::ToolText, Sink::ActionArgument, &[]));
    // log przyjmuje wszystko (to nie steruje akcją)
    assert!(przeplyw_ok(Zaufanie::ToolText, Sink::Log, &[]));
}

#[test]
fn subject_dlugosc_i_enumeracja() {
    // obrony wartości: zbyt długi subject, nieznana akcja
    let s = sufit();
    let dlugi = json!({"to": "team@example.com", "subject": "x".repeat(101)});
    assert!(!czy_wolno(&s, "send_email", &dlugi, Zaufanie::Operator, &[]));
    assert!(!czy_wolno(&s, "shell", &json!({"cmd": "id"}), Zaufanie::Operator, &[]));
}

#[test]
fn single_use_grant_composes_with_value_policy() {
    // #6 (permission race): grant single-use. Tu sprawdzamy tylko, że provenance musi
    // być operatora; pełny grant (Zezwolenie) testuje `tamper_matrix.rs`.
    let s = sufit();
    let w = json!({"to": "boss@example.com", "subject": "ok"});
    assert!(czy_wolno(&s, "send_email", &w, Zaufanie::Operator, &[]));
    // jeśli wartość pochodzi z injection, nie wolno — niezależnie od tego, że akcja istnieje
    assert!(!czy_wolno(&s, "send_email", &w, Zaufanie::ModelDerived, &[]));
}
