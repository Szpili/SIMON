//! I2 (M7) — **capability ceiling**: LLM wypełnia sloty, nie definiuje akcji.
//!
//! To NIE filtr treści (krytyk: „system prompt mówi, że to dane" = teatr). To
//! struktura: model może zaproponować akcję **tylko z sufitu**, a każdy parametr
//! jest walidowany (typ/długość/enum/zakres). Akcja spoza sufitu i parametr poza
//! kontraktem są odrzucane, zanim cokolwiek zostanie wykonane.
//!
//! Model może PROSIĆ o `log_message`; nie może „poprosić" o `shell`. Podszycie
//! się pod akcję parametrem (np. `{"tool":"shell","args":{"cmd":"rm -rf /"}}`)
//! kończy się `NieznanaAkcja`, a nie wykonaniem.
//!
//! Ewaluacja/„Zezwolenie" operatora (I3) jest wyżej; tu jest wyłącznie bramka
//! kształtu i zakresu (CPU, bez modelu).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Ograniczenie pojedynczego parametru akcji.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum Ograniczenie {
    Tekst { max_len: usize },
    Enum { wartosci: Vec<String> },
    Zakres { min: i64, max: i64 },
    Bool,
}

/// Deklaracja parametru (część kontraktu akcji).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parametr {
    pub nazwa: String,
    pub wymagany: bool,
    pub ograniczenie: Ograniczenie,
}

/// Akcja dozwolona przez operatora (capability ceiling).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Akcja {
    pub nazwa: String,
    pub parametry: Vec<Parametr>,
}

/// Sufit: pełny zbiór akcji, które harness może w ogóle rozważać.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sufit {
    pub akcje: BTreeMap<String, Akcja>,
}

/// Powód odrzucenia propozycji modelu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "odmowa", rename_all = "snake_case")]
pub enum Odmowa {
    NieznanaAkcja { akcja: String },
    BrakParametru { akcja: String, parametr: String },
    NadmiarowyParametr { akcja: String, parametr: String },
    ZlyTyp { akcja: String, parametr: String },
    ZaDlugi { akcja: String, parametr: String, max: usize },
    PozaEnum { akcja: String, parametr: String },
    PozaZakresem { akcja: String, parametr: String },
}

impl std::fmt::Display for Odmowa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Odmowa::NieznanaAkcja { akcja } => write!(f, "akcja spoza sufitu: {akcja}"),
            Odmowa::BrakParametru { akcja, parametr } => {
                write!(f, "{akcja}: brak wymaganego parametru {parametr}")
            }
            Odmowa::NadmiarowyParametr { akcja, parametr } => {
                write!(f, "{akcja}: parametr spoza kontraktu: {parametr}")
            }
            Odmowa::ZlyTyp { akcja, parametr } => write!(f, "{akcja}: zły typ parametru {parametr}"),
            Odmowa::ZaDlugi { akcja, parametr, max } => {
                write!(f, "{akcja}: parametr {parametr} dłuższy niż {max}")
            }
            Odmowa::PozaEnum { akcja, parametr } => {
                write!(f, "{akcja}: parametr {parametr} poza dozwolonym zbiorem")
            }
            Odmowa::PozaZakresem { akcja, parametr } => {
                write!(f, "{akcja}: parametr {parametr} poza zakresem")
            }
        }
    }
}

/// Waliduje propozycję modelu wobec sufitu. Ścisła: akcja musi istnieć,
/// wszystkie wymagane parametry obecne, żadnych nadmiarowych, każdy w zakresie.
pub fn waliduj(sufit: &Sufit, akcja: &str, parametry: &Value) -> Result<(), Odmowa> {
    let def = sufit
        .akcje
        .get(akcja)
        .ok_or_else(|| Odmowa::NieznanaAkcja { akcja: akcja.to_string() })?;

    let obj = parametry.as_object();
    // Nadmiarowe parametry: cokolwiek spoza kontraktu.
    if let Some(o) = obj {
        for k in o.keys() {
            if !def.parametry.iter().any(|p| &p.nazwa == k) {
                return Err(Odmowa::NadmiarowyParametr {
                    akcja: akcja.to_string(),
                    parametr: k.clone(),
                });
            }
        }
    }

    for p in &def.parametry {
        let v = obj.and_then(|o| o.get(&p.nazwa));
        let Some(v) = v else {
            if p.wymagany {
                return Err(Odmowa::BrakParametru {
                    akcja: akcja.to_string(),
                    parametr: p.nazwa.clone(),
                });
            }
            continue;
        };
        let zle = |odm: Odmowa| Err(odm);
        match &p.ograniczenie {
            Ograniczenie::Tekst { max_len } => match v.as_str() {
                Some(s) if s.chars().count() <= *max_len => {}
                Some(_) => {
                    return zle(Odmowa::ZaDlugi {
                        akcja: akcja.to_string(),
                        parametr: p.nazwa.clone(),
                        max: *max_len,
                    })
                }
                None => {
                    return zle(Odmowa::ZlyTyp {
                        akcja: akcja.to_string(),
                        parametr: p.nazwa.clone(),
                    })
                }
            },
            Ograniczenie::Enum { wartosci } => match v.as_str() {
                Some(s) if wartosci.iter().any(|w| w == s) => {}
                Some(_) => {
                    return zle(Odmowa::PozaEnum {
                        akcja: akcja.to_string(),
                        parametr: p.nazwa.clone(),
                    })
                }
                None => {
                    return zle(Odmowa::ZlyTyp {
                        akcja: akcja.to_string(),
                        parametr: p.nazwa.clone(),
                    })
                }
            },
            Ograniczenie::Zakres { min, max } => match v.as_i64() {
                Some(n) if n >= *min && n <= *max => {}
                Some(_) => {
                    return zle(Odmowa::PozaZakresem {
                        akcja: akcja.to_string(),
                        parametr: p.nazwa.clone(),
                    })
                }
                None => {
                    return zle(Odmowa::ZlyTyp {
                        akcja: akcja.to_string(),
                        parametr: p.nazwa.clone(),
                    })
                }
            },
            Ograniczenie::Bool => {
                if !v.is_boolean() {
                    return zle(Odmowa::ZlyTyp {
                        akcja: akcja.to_string(),
                        parametr: p.nazwa.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Propozycja akcji od modelu: `{"tool": "<akcja>", "args": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Propozycja {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

/// Waliduje surową propozycję JSON (`{"tool":..,"args":{..}}`) wobec sufitu.
pub fn waliduj_json(sufit: &Sufit, propozycja_json: &str) -> Result<(), Odmowa> {
    let p: Propozycja = serde_json::from_str(propozycja_json)
        .map_err(|_| Odmowa::ZlyTyp { akcja: "<json>".into(), parametr: "args".into() })?;
    waliduj(sufit, &p.tool, &p.args)
}

#[cfg(test)]
mod testy {
    use super::*;

    fn sufit() -> Sufit {
        let mut akcje = BTreeMap::new();
        akcje.insert(
            "log_message".to_string(),
            Akcja {
                nazwa: "log_message".into(),
                parametry: vec![Parametr {
                    nazwa: "msg".into(),
                    wymagany: true,
                    ograniczenie: Ograniczenie::Tekst { max_len: 200 },
                }],
            },
        );
        akcje.insert(
            "ustaw_tryb".to_string(),
            Akcja {
                nazwa: "ustaw_tryb".into(),
                parametry: vec![
                    Parametr {
                        nazwa: "tryb".into(),
                        wymagany: true,
                        ograniczenie: Ograniczenie::Enum {
                            wartosci: vec!["safe".into(), "fast".into()],
                        },
                    },
                    Parametr {
                        nazwa: "limit".into(),
                        wymagany: false,
                        ograniczenie: Ograniczenie::Zakres { min: 0, max: 10 },
                    },
                ],
            },
        );
        Sufit { akcje }
    }

    /// Najtańszy test z krytyki: model próbuje akcji spoza sufitu z payloadem
    /// w parametrze. Sufit nie zna `shell` → odrzucone, nic się nie wykonuje.
    #[test]
    fn malicious_json_parameter_nie_wchodzi() {
        let s = sufit();
        let zle = r#"{"tool":"shell","args":{"cmd":"rm -rf / --no-preserve-root"}}"#;
        assert!(matches!(waliduj_json(&s, zle), Err(Odmowa::NieznanaAkcja { .. })));

        // To samo jako bezpieczny slot: `log_message` z tym samym tekstem przechodzi
        // — bo akcja jest nieszkodliwa, a treść to DANE, nie akcja.
        let ok = r#"{"tool":"log_message","args":{"msg":"rm -rf / --no-preserve-root"}}"#;
        assert!(waliduj_json(&s, ok).is_ok());
    }

    #[test]
    fn brak_i_nadmiarowy_parametr_odrzucone() {
        let s = sufit();
        assert!(matches!(
            waliduj_json(&s, r#"{"tool":"log_message","args":{}}"#),
            Err(Odmowa::BrakParametru { .. })
        ));
        assert!(matches!(
            waliduj_json(&s, r#"{"tool":"log_message","args":{"msg":"hi","x":1}}"#),
            Err(Odmowa::NadmiarowyParametr { .. })
        ));
    }

    #[test]
    fn enum_zly_typ_zakres_dlugosc() {
        let s = sufit();
        assert!(matches!(
            waliduj_json(&s, r#"{"tool":"ustaw_tryb","args":{"tryb":"yolo"}}"#),
            Err(Odmowa::PozaEnum { .. })
        ));
        assert!(matches!(
            waliduj_json(&s, r#"{"tool":"ustaw_tryb","args":{"tryb":"safe","limit":99}}"#),
            Err(Odmowa::PozaZakresem { .. })
        ));
        assert!(matches!(
            waliduj_json(&s, r#"{"tool":"log_message","args":{"msg":42}}"#),
            Err(Odmowa::ZlyTyp { .. })
        ));
        let dlugi = "a".repeat(201);
        assert!(matches!(
            waliduj_json(&s, &format!(r#"{{"tool":"log_message","args":{{"msg":"{dlugi}"}}}}"#)),
            Err(Odmowa::ZaDlugi { .. })
        ));
        // opcjonalny `limit` pominięty — OK
        assert!(waliduj_json(&s, r#"{"tool":"ustaw_tryb","args":{"tryb":"fast"}}"#).is_ok());
    }
}
