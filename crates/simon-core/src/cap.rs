//! I2 (M7) — **capability ceiling**: LLM fills slots, it does not define actions.
//!
//! This is NOT content filtering. This is structure: the model may propose an action
//! **only from the ceiling**, and each parameter is validated (type, length, enum, range).
//! Actions outside the ceiling and parameters outside contract are rejected before execution.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::crypto::{Keypair, PublicKey, Signature};

/// Constraint on an individual action parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum ParamConstraint {
    Tekst { max_len: usize },
    #[serde(alias = "text")]
    Text { max_len: usize },
    Enum {
        #[serde(alias = "wartosci")]
        values: Vec<String>,
    },
    #[serde(alias = "zakres")]
    Range { min: i64, max: i64 },
    Bool,
}

pub type Ograniczenie = ParamConstraint;

/// Parameter specification within an action contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionParam {
    #[serde(alias = "nazwa")]
    pub name: String,
    #[serde(alias = "wymagany")]
    pub required: bool,
    #[serde(alias = "ograniczenie")]
    pub constraint: ParamConstraint,
}

pub type Parametr = ActionParam;

impl ActionParam {
    pub fn nazwa(&self) -> &str {
        &self.name
    }
    pub fn wymagany(&self) -> bool {
        self.required
    }
}

/// Permitted action definition (capability ceiling).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDef {
    #[serde(alias = "nazwa")]
    pub name: String,
    #[serde(alias = "parametry")]
    pub params: Vec<ActionParam>,
}

pub type Akcja = ActionDef;

/// Capability ceiling: total set of actions the harness can consider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityCeiling {
    #[serde(alias = "akcje")]
    pub actions: BTreeMap<String, ActionDef>,
}

/// Reason for rejecting a model's action proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rejection", rename_all = "snake_case")]
pub enum Rejection {
    UnknownAction { action: String },
    MissingParam { action: String, param: String },
    ExtraParam { action: String, param: String },
    BadType { action: String, param: String },
    TooLong { action: String, param: String, max: usize },
    NotInEnum { action: String, param: String },
    OutOfRange { action: String, param: String },
}

pub type Odmowa = Rejection;

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rejection::UnknownAction { action } => write!(f, "action outside ceiling: {action}"),
            Rejection::MissingParam { action, param } => {
                write!(f, "{action}: missing required parameter {param}")
            }
            Rejection::ExtraParam { action, param } => {
                write!(f, "{action}: parameter outside contract: {param}")
            }
            Rejection::BadType { action, param } => write!(f, "{action}: bad type for parameter {param}"),
            Rejection::TooLong { action, param, max } => {
                write!(f, "{action}: parameter {param} exceeds max length {max}")
            }
            Rejection::NotInEnum { action, param } => {
                write!(f, "{action}: parameter {param} not in allowed enum values")
            }
            Rejection::OutOfRange { action, param } => {
                write!(f, "{action}: parameter {param} out of allowed range")
            }
        }
    }
}

/// Validates a model proposal against the capability ceiling.
pub fn validate(ceiling: &CapabilityCeiling, action: &str, params: &Value) -> Result<(), Rejection> {
    let def = ceiling
        .actions
        .get(action)
        .ok_or_else(|| Rejection::UnknownAction { action: action.to_string() })?;

    let empty = serde_json::Map::new();
    let o = match params {
        Value::Null => &empty,
        Value::Object(o) => o,
        _ => {
            return Err(Rejection::BadType {
                action: action.to_string(),
                param: "params".to_string(),
            })
        }
    };

    for k in o.keys() {
        if !def.params.iter().any(|p| &p.name == k) {
            return Err(Rejection::ExtraParam {
                action: action.to_string(),
                param: k.clone(),
            });
        }
    }

    for p in &def.params {
        let v = o.get(&p.name);
        let Some(v) = v else {
            if p.required {
                return Err(Rejection::MissingParam {
                    action: action.to_string(),
                    param: p.name.clone(),
                });
            }
            continue;
        };

        match &p.constraint {
            ParamConstraint::Tekst { max_len } | ParamConstraint::Text { max_len } => match v.as_str() {
                Some(s) if s.chars().count() <= *max_len => {}
                Some(_) => {
                    return Err(Rejection::TooLong {
                        action: action.to_string(),
                        param: p.name.clone(),
                        max: *max_len,
                    })
                }
                None => {
                    return Err(Rejection::BadType {
                        action: action.to_string(),
                        param: p.name.clone(),
                    })
                }
            },
            ParamConstraint::Enum { values } => match v.as_str() {
                Some(s) if values.iter().any(|w| w == s) => {}
                Some(_) => {
                    return Err(Rejection::NotInEnum {
                        action: action.to_string(),
                        param: p.name.clone(),
                    })
                }
                None => {
                    return Err(Rejection::BadType {
                        action: action.to_string(),
                        param: p.name.clone(),
                    })
                }
            },
            ParamConstraint::Range { min, max } => match v.as_i64() {
                Some(n) if n >= *min && n <= *max => {}
                Some(_) => {
                    return Err(Rejection::OutOfRange {
                        action: action.to_string(),
                        param: p.name.clone(),
                    })
                }
                None => {
                    return Err(Rejection::BadType {
                        action: action.to_string(),
                        param: p.name.clone(),
                    })
                }
            },
            ParamConstraint::Bool => {
                if !v.is_boolean() {
                    return Err(Rejection::BadType {
                        action: action.to_string(),
                        param: p.name.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

pub use validate as waliduj;

/// Model action proposal: `{"tool": "<action>", "args": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

pub type Propozycja = Proposal;

/// Validates raw JSON proposal against the capability ceiling.
pub fn validate_json(ceiling: &CapabilityCeiling, proposal_json: &str) -> Result<(), Rejection> {
    let p: Proposal = serde_json::from_str(proposal_json)
        .map_err(|_| Rejection::BadType { action: "<json>".into(), param: "args".into() })?;
    validate(ceiling, &p.tool, &p.args)
}

pub use validate_json as waliduj_json;

/// I3 (M7) — **Capability grant**. Operator authorization for single-use action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub id: String,
    #[serde(rename = "action", alias = "akcja")]
    pub action: String,
    #[serde(rename = "params_digest", alias = "parametry_digest")]
    pub params_digest: String,
    #[serde(rename = "issuer", alias = "wystawca")]
    pub issuer: String,
    #[serde(rename = "valid_until_us", alias = "wazne_do_us")]
    pub valid_until_us: u64,
    #[serde(rename = "budget", alias = "budzet", skip_serializing_if = "Option::is_none", default)]
    pub budget: Option<i64>,
    #[serde(rename = "signature", alias = "podpis", skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
}

/// Canonical digest of action parameters.
pub fn action_params_digest(action: &str, params: &Value) -> Result<String, crate::SimonError> {
    crate::content_digest(&serde_json::json!({
        "domena": "SIMON/GRANT-PARAMS/v1",
        "action": action,
        "params": params,
    }))
}

pub fn odcisk_parametrow_akcji(akcja: &str, parametry: &Value) -> Result<String, crate::SimonError> {
    crate::content_digest(&serde_json::json!({
        "domena": "SIMON/GRANT-PARAMS/v1",
        "akcja": akcja,
        "parametry": parametry,
    }))
}

impl CapabilityGrant {
    pub fn digest(&self) -> Result<String, crate::SimonError> {
        let mut z = self.clone();
        z.signature = None;
        crate::content_digest(&z)
    }

    pub fn sign(mut self, kp: &Keypair) -> Result<Self, crate::SimonError> {
        self.issuer = kp.public().to_hex();
        let d = self.digest()?;
        self.signature = Some(hex::encode(kp.sign_digest(&d).0));
        Ok(self)
    }

    pub fn podpisz(self, kp: &Keypair) -> Result<Self, crate::SimonError> {
        self.sign(kp)
    }
}

/// Grant evaluation context.
pub struct GrantContext<'a> {
    pub now_us: u64,
    pub expected_issuer: &'a str,
    pub revoked: &'a [String],
    pub used: &'a [String],
}

pub type Kontekst<'a> = GrantContext<'a>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantRejection {
    BadFormat,
    Signature,
    Issuer,
    Expired,
    Revoked,
    Used,
    BadAction,
    Params,
}

pub type OdmowaZ = GrantRejection;

/// Verifies a grant against action, parameters, and context.
pub fn verify_grant(
    ceiling: &CapabilityCeiling,
    grant: &CapabilityGrant,
    action: &str,
    params: &Value,
    ctx: &GrantContext<'_>,
) -> Result<(), GrantRejection> {
    if grant.id.is_empty() {
        return Err(GrantRejection::BadFormat);
    }
    let pk = PublicKey::from_hex(&grant.issuer).map_err(|_| GrantRejection::BadFormat)?;
    let sig_hex = grant.signature.as_ref().ok_or(GrantRejection::Signature)?;
    let raw = hex::decode(sig_hex).map_err(|_| GrantRejection::BadFormat)?;
    let arr: [u8; 64] = raw.try_into().map_err(|_| GrantRejection::BadFormat)?;
    let digest = grant.digest().map_err(|_| GrantRejection::BadFormat)?;
    pk.verify_digest(&digest, &Signature(arr))
        .map_err(|_| GrantRejection::Signature)?;

    if grant.issuer != ctx.expected_issuer {
        return Err(GrantRejection::Issuer);
    }
    if ctx.now_us > grant.valid_until_us {
        return Err(GrantRejection::Expired);
    }
    if let Some(b) = grant.budget {
        if b <= 0 {
            return Err(GrantRejection::Expired);
        }
    }
    if ctx.revoked.iter().any(|x| x == &grant.id) {
        return Err(GrantRejection::Revoked);
    }
    if ctx.used.iter().any(|x| x == &grant.id) {
        return Err(GrantRejection::Used);
    }
    if grant.action != action {
        return Err(GrantRejection::BadAction);
    }
    let d1 = action_params_digest(action, params).map_err(|_| GrantRejection::BadFormat)?;
    let d2 = odcisk_parametrow_akcji(action, params).map_err(|_| GrantRejection::BadFormat)?;
    if d1 != grant.params_digest && d2 != grant.params_digest {
        return Err(GrantRejection::Params);
    }
    validate(ceiling, action, params).map_err(|_| GrantRejection::Params)?;
    Ok(())
}

pub use verify_grant as sprawdz;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ceiling() -> CapabilityCeiling {
        let mut actions = BTreeMap::new();
        actions.insert(
            "log_message".to_string(),
            ActionDef {
                name: "log_message".into(),
                params: vec![ActionParam {
                    name: "msg".into(),
                    required: true,
                    constraint: ParamConstraint::Text { max_len: 200 },
                }],
            },
        );
        actions.insert(
            "set_mode".to_string(),
            ActionDef {
                name: "set_mode".into(),
                params: vec![
                    ActionParam {
                        name: "mode".into(),
                        required: true,
                        constraint: ParamConstraint::Enum {
                            values: vec!["safe".into(), "fast".into()],
                        },
                    },
                    ActionParam {
                        name: "limit".into(),
                        required: false,
                        constraint: ParamConstraint::Range { min: 0, max: 10 },
                    },
                ],
            },
        );
        CapabilityCeiling { actions }
    }

    #[test]
    fn malicious_json_parameter_rejected() {
        let s = test_ceiling();
        let bad = r#"{"tool":"shell","args":{"cmd":"rm -rf / --no-preserve-root"}}"#;
        assert!(matches!(validate_json(&s, bad), Err(Rejection::UnknownAction { .. })));

        let ok = r#"{"tool":"log_message","args":{"msg":"rm -rf / --no-preserve-root"}}"#;
        assert!(validate_json(&s, ok).is_ok());
    }

    #[test]
    fn missing_and_extra_param_rejected() {
        let s = test_ceiling();
        assert!(matches!(
            validate_json(&s, r#"{"tool":"log_message","args":{}}"#),
            Err(Rejection::MissingParam { .. })
        ));
        assert!(matches!(
            validate_json(&s, r#"{"tool":"log_message","args":{"msg":"hi","x":1}}"#),
            Err(Rejection::ExtraParam { .. })
        ));
    }

    #[test]
    fn grant_verifies_and_fails_each_gate() {
        let kp = Keypair::from_seed(&[77u8; 32]);
        let s = test_ceiling();
        let action = "log_message";
        let par = serde_json::json!({"msg": "hello"});
        let g = CapabilityGrant {
            id: "grant-1".into(),
            action: action.into(),
            params_digest: action_params_digest(action, &par).unwrap(),
            issuer: "".into(),
            valid_until_us: 1_000_000,
            budget: Some(10),
            signature: None,
        }
        .sign(&kp)
        .unwrap();

        let ctx = GrantContext {
            now_us: 500_000,
            expected_issuer: &kp.public().to_hex(),
            revoked: &[],
            used: &[],
        };
        assert!(verify_grant(&s, &g, action, &par, &ctx).is_ok());

        // Gate 1: Tampered parameter value
        let par2 = serde_json::json!({"msg": "tampered"});
        assert_eq!(
            verify_grant(&s, &g, action, &par2, &ctx),
            Err(GrantRejection::Params)
        );

        // Gate 2: Action mismatch
        assert_eq!(
            verify_grant(&s, &g, "set_mode", &par, &ctx),
            Err(GrantRejection::BadAction)
        );

        // Gate 3: Expired
        let mut ctx_exp = ctx;
        ctx_exp.now_us = 2_000_000;
        assert_eq!(
            verify_grant(&s, &g, action, &par, &ctx_exp),
            Err(GrantRejection::Expired)
        );
    }
}
