//! Tests for value policies and data flow boundaries.
//! Validates values against CapabilityCeiling AND provenance against DataSink.

use serde_json::json;
use simon_core::cap::{validate, ActionDef, ActionParam, CapabilityCeiling, ParamConstraint};
use simon_core::flow::{flow_ok, DataSink, TrustLevel};

fn ceiling() -> CapabilityCeiling {
    let mut actions = std::collections::BTreeMap::new();
    actions.insert(
        "send_email".into(),
        ActionDef {
            name: "send_email".into(),
            params: vec![
                ActionParam {
                    name: "to".into(),
                    required: true,
                    // Recipient allowlist — action name alone is insufficient.
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

/// Full decision for action: value validation AND provenance to sink.
fn is_allowed(
    s: &CapabilityCeiling,
    action: &str,
    values: &serde_json::Value,
    source: TrustLevel,
    declassifications: &[(TrustLevel, DataSink)],
) -> bool {
    if validate(s, action, values).is_err() {
        return false;
    }
    flow_ok(source, DataSink::ActionArgument, declassifications)
}

#[test]
fn value_allowlist_rejects_unauthorized_recipient() {
    let s = ceiling();
    let w = json!({"to": "attacker@evil.com", "subject": "hi"});
    assert!(!is_allowed(&s, "send_email", &w, TrustLevel::Operator, &[]));

    let ok = json!({"to": "boss@example.com", "subject": "hi"});
    assert!(is_allowed(&s, "send_email", &ok, TrustLevel::Operator, &[]));
}

#[test]
fn valid_value_from_untrusted_source_is_rejected() {
    let s = ceiling();
    let w = json!({"to": "boss@example.com", "subject": "summary"});
    for source in [
        TrustLevel::RetrievedText,
        TrustLevel::PeerText,
        TrustLevel::ToolText,
        TrustLevel::ModelDerived,
    ] {
        assert!(
            !is_allowed(&s, "send_email", &w, source, &[]),
            "{source:?} cannot control recipient without explicit declassification"
        );
        assert!(is_allowed(&s, "send_email", &w, source, &[(source, DataSink::ActionArgument)]));
    }
}

#[test]
fn tool_result_injection_does_not_control_action() {
    assert!(!flow_ok(TrustLevel::ToolText, DataSink::ActionArgument, &[]));
    assert!(flow_ok(TrustLevel::ToolText, DataSink::Log, &[]));
}

#[test]
fn subject_length_and_enumeration() {
    let s = ceiling();
    let long_subj = json!({"to": "team@example.com", "subject": "x".repeat(101)});
    assert!(!is_allowed(&s, "send_email", &long_subj, TrustLevel::Operator, &[]));
    assert!(!is_allowed(&s, "shell", &json!({"cmd": "id"}), TrustLevel::Operator, &[]));
}

#[test]
fn single_use_grant_composes_with_value_policy() {
    let s = ceiling();
    let w = json!({"to": "boss@example.com", "subject": "ok"});
    assert!(is_allowed(&s, "send_email", &w, TrustLevel::Operator, &[]));
    assert!(!is_allowed(&s, "send_email", &w, TrustLevel::ModelDerived, &[]));
}
