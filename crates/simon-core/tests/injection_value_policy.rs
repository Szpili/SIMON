//! Testy na „10 sposobów złamania granicy w 10 minut" (ocena hackerska 2026-10-05).
//! Sedno: sama nazwa akcji to za mało — liczą się **wartości** i ich **pochodzenie**.
//! `waliduj` sprawdza kształt/zakres wartości; `przeplyw_ok` sprawdza provenance.
//! Decyzja = koniunkcja: (akcja+wartości OK) ORAZ (provenance → sink dozwolone).

use serde_json::json;
use simon_core::cap::{waliduj, Akcja, Ograniczenie, Parametr, Sufit};
use simon_core::przeplyw::{przeplyw_ok, Sink, Zaufanie};

fn sufit() -> Sufit {
    let mut akcje = std::collections::BTreeMap::new();
    akcje.insert(
        "send_email".into(),
        Akcja {
            nazwa: "send_email".into(),
            parametry: vec![
                Parametr {
                    nazwa: "to".into(),
                    wymagany: true,
                    // Allowlist adresatów — sama nazwa akcji NIE wystarcza.
                    ograniczenie: Ograniczenie::Enum {
                        wartosci: vec!["boss@example.com".into(), "team@example.com".into()],
                    },
                },
                Parametr {
                    nazwa: "subject".into(),
                    wymagany: true,
                    ograniczenie: Ograniczenie::Tekst { max_len: 100 },
                },
            ],
        },
    );
    Sufit { akcje }
}

/// Pełna decyzja dla akcji: kształt wartości (I2) ORAZ provenance do sinku (I4).
fn czy_wolno(
    s: &Sufit,
    akcja: &str,
    wartosci: &serde_json::Value,
    zrodlo: Zaufanie,
    deklasacje: &[(Zaufanie, Sink)],
) -> bool {
    if waliduj(s, akcja, wartosci).is_err() {
        return false;
    }
    przeplyw_ok(zrodlo, Sink::ArgumentAkcji, deklasacje)
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
        assert!(czy_wolno(&s, "send_email", &w, zrodlo, &[(zrodlo, Sink::ArgumentAkcji)]));
    }
}

#[test]
fn tool_result_injection_nie_steruje_akcja() {
    // #2: wstrzyknięcie w wyniku narzędzia. ToolText → ArgumentAkcji zawsze deny.
    assert!(!przeplyw_ok(Zaufanie::ToolText, Sink::ArgumentAkcji, &[]));
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
