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

use crate::crypto::{Keypair, PublicKey, Signature};

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

/// I3 (M7) — **Zezwolenie operatora**. Model może PROSIĆ; harness wykonuje tylko po
/// jawnym, świeżym i zwalidowanym zezwoleniu. Wymagania z krytyki (bunny): krypto-wiązane
/// z tożsamością klienta, zakres dokładnej akcji+parametrów, TTL, budżet, single-use
/// (replay-safe), odwoływalne, wymuszone na granicy zdolności (nie „ambient authority").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zezwolenie {
    /// Unikalny identyfikator (nonce) — single-use / replay-safe.
    pub id: String,
    /// Dokładna nazwa akcji z sufitu.
    pub akcja: String,
    /// Odcisk ZATWIERDZONYCH parametrów (operator zatwierdza WARTOŚCI, nie typ).
    pub parametry_digest: String,
    /// Klucz publiczny wystawcy (hex) — tożsamość klienta/operatora.
    pub wystawca: String,
    /// TTL (mikrosekundy, zegar wystawcy).
    pub wazne_do_us: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub budzet: Option<i64>,
    /// Podpis Ed25519 po odcisku zezwolenia. Wyłączony z odcisku.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub podpis: Option<String>,
}

/// Kanoniczny odcisk parametrów akcji (co dokładnie zatwierdzono).
pub fn odcisk_parametrow_akcji(akcja: &str, parametry: &Value) -> Result<String, crate::SimonError> {
    crate::content_digest(&serde_json::json!({
        "domena": "SIMON/GRANT-PARAMS/v1",
        "akcja": akcja,
        "parametry": parametry,
    }))
}

impl Zezwolenie {
    /// Odcisk zezwolenia (podpis nie wchodzi).
    pub fn digest(&self) -> Result<String, crate::SimonError> {
        let mut z = self.clone();
        z.podpis = None;
        crate::content_digest(&z)
    }

    /// Podpisuje zezwolenie kluczem operatora (ustawia `wystawca`).
    pub fn podpisz(mut self, kp: &Keypair) -> Result<Self, crate::SimonError> {
        self.wystawca = kp.public().to_hex();
        let d = self.digest()?;
        self.podpis = Some(hex::encode(kp.sign_digest(&d).0));
        Ok(self)
    }
}

/// Kontekst sprawdzenia zezwolenia (stan bieżący).
pub struct Kontekst<'a> {
    pub teraz_us: u64,
    /// Oczekiwana tożsamość operatora (hex) — zaufana strona.
    pub oczekiwany_wystawca: &'a str,
    /// Id zezwoleń odwołanych.
    pub odwolane: &'a [String],
    /// Id zezwoleń już użytych (single-use).
    pub uzyte: &'a [String],
}

/// Powód odrzucenia zezwolenia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OdmowaZ {
    ZlyFormat,
    Podpis,
    Wystawca,
    Wygaslo,
    Odwolane,
    Uzyte,
    ZlaAkcja,
    Parametry,
}

/// Sprawdza zezwolenie wobec akcji, parametrów i kontekstu. Wszystkie bramki muszą przejść.
pub fn sprawdz(
    sufit: &Sufit,
    z: &Zezwolenie,
    akcja: &str,
    parametry: &Value,
    k: &Kontekst<'_>,
) -> Result<(), OdmowaZ> {
    if z.id.is_empty() {
        return Err(OdmowaZ::ZlyFormat);
    }
    // 1) podpis pod kluczem wystawcy
    let pk = PublicKey::from_hex(&z.wystawca).map_err(|_| OdmowaZ::ZlyFormat)?;
    let sig_hex = z.podpis.as_ref().ok_or(OdmowaZ::Podpis)?;
    let raw = hex::decode(sig_hex).map_err(|_| OdmowaZ::ZlyFormat)?;
    let arr: [u8; 64] = raw.try_into().map_err(|_| OdmowaZ::ZlyFormat)?;
    let digest = z.digest().map_err(|_| OdmowaZ::ZlyFormat)?;
    pk.verify_digest(&digest, &Signature(arr))
        .map_err(|_| OdmowaZ::Podpis)?;
    // 2) tożsamość wystawcy
    if z.wystawca != k.oczekiwany_wystawca {
        return Err(OdmowaZ::Wystawca);
    }
    // 3) TTL
    if k.teraz_us > z.wazne_do_us {
        return Err(OdmowaZ::Wygaslo);
    }
    // 4) odwołane / użyte
    if k.odwolane.iter().any(|x| x == &z.id) {
        return Err(OdmowaZ::Odwolane);
    }
    if k.uzyte.iter().any(|x| x == &z.id) {
        return Err(OdmowaZ::Uzyte);
    }
    // 5) dokładna akcja i parametry (zatwierdzone WARTOŚCI)
    if z.akcja != akcja {
        return Err(OdmowaZ::ZlaAkcja);
    }
    let d = odcisk_parametrow_akcji(akcja, parametry).map_err(|_| OdmowaZ::ZlyFormat)?;
    if d != z.parametry_digest {
        return Err(OdmowaZ::Parametry);
    }
    // 6) i muszą przechodzić capability ceiling
    waliduj(sufit, akcja, parametry).map_err(|_| OdmowaZ::Parametry)?;
    Ok(())
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
    fn zezwolenie_dziala_i_lamie_sie_na_kazdej_bramce() {
        let s = sufit();
        let op = Keypair::from_seed(&[5u8; 32]);
        let pk_hex = op.public().to_hex();
        let inny_hex = Keypair::from_seed(&[6u8; 32]).public().to_hex();
        let akcja = "log_message";
        let par = serde_json::json!({"msg": "hello"});
        let z = Zezwolenie {
            id: "g-1".into(),
            akcja: akcja.into(),
            parametry_digest: odcisk_parametrow_akcji(akcja, &par).unwrap(),
            wystawca: String::new(),
            wazne_do_us: 1_000_000,
            budzet: Some(10),
            podpis: None,
        }
        .podpisz(&op)
        .unwrap();

        fn kk<'a>(
            teraz: u64,
            w: &'a str,
            odwol: &'a [String],
            uzyte: &'a [String],
        ) -> Kontekst<'a> {
            Kontekst {
                teraz_us: teraz,
                oczekiwany_wystawca: w,
                odwolane: odwol,
                uzyte,
            }
        }
        let odw: Vec<String> = vec!["g-1".into()];

        // ścieżka szczęśliwa
        assert!(sprawdz(&s, &z, akcja, &par, &kk(500_000, &pk_hex, &[], &[])).is_ok());
        // inne parametry — operator zatwierdził WARTOŚCI
        let inne = serde_json::json!({"msg": "rm -rf /"});
        assert_eq!(
            sprawdz(&s, &z, akcja, &inne, &kk(500_000, &pk_hex, &[], &[])),
            Err(OdmowaZ::Parametry)
        );
        // inna akcja
        assert_eq!(
            sprawdz(&s, &z, "shell", &par, &kk(500_000, &pk_hex, &[], &[])),
            Err(OdmowaZ::ZlaAkcja)
        );
        // wygasło
        assert_eq!(
            sprawdz(&s, &z, akcja, &par, &kk(2_000_000, &pk_hex, &[], &[])),
            Err(OdmowaZ::Wygaslo)
        );
        // odwołane
        assert_eq!(
            sprawdz(&s, &z, akcja, &par, &kk(500_000, &pk_hex, &odw, &[])),
            Err(OdmowaZ::Odwolane)
        );
        // użyte (single-use)
        assert_eq!(
            sprawdz(&s, &z, akcja, &par, &kk(500_000, &pk_hex, &[], &odw)),
            Err(OdmowaZ::Uzyte)
        );
        // zły wystawca
        assert_eq!(
            sprawdz(&s, &z, akcja, &par, &kk(500_000, &inny_hex, &[], &[])),
            Err(OdmowaZ::Wystawca)
        );
        // podmieniony podpis
        let mut z2 = z.clone();
        z2.podpis = Some("00".repeat(64));
        assert_eq!(
            sprawdz(&s, &z2, akcja, &par, &kk(500_000, &pk_hex, &[], &[])),
            Err(OdmowaZ::Podpis)
        );
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
