//! M3 — weryfikacja wykonania przez **containment w top-k**.
//!
//! Podstawa pomiarowa (brain: `simon-e0-fp-divergence`, `simon-m3-topk-kalibracja`):
//! - exact-match CPU↔GPU przy temp=0 jest MARTWY (dywergencja od ~3 tokenów),
//! - dywergencja FP to zawsze spór top-1 vs top-2 (margines logprob 0,07–0,17),
//! - gdy verifier PODĄŻA trajektorią node'a (prefix-forcing po tokenach),
//!   token node'a jest w top-10 w 150/150 krokach, a w top-2 w 100%.
//!
//! Wniosek: nie porównujemy równości tokenów. Verifier odtwarza krok po kroku
//! rozkład top-k i akceptuje token node'a, jeśli mieści się w top-k w granicach
//! marginesu. To jest warstwa DECYZYJNA — sama inferencja verifiera (llama.cpp/
//! vLLM) jest poza tym modułem; tu wchodzą gotowe kroki (token + top-k).

use serde::{Deserialize, Serialize};

use crate::{content_digest, SimonError};

/// Separator domeny odcisku audytu.
pub const DOMENA_AUDYTU: &str = "SIMON/AUDIT/v1";

/// Jeden sprawdzony krok: token node'a vs rozkład verifiera w tym kroku.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Krok {
    pub indeks: u32,
    /// Token wybrany przez node'a (z jego `output_token_ids`).
    pub node_token: u32,
    /// Top-k verifiera: `(token_id, logprob)`, posortowane malejąco po logprob.
    pub topk: Vec<(u32, f64)>,
}

/// Polityka akceptacji. Domyślne wartości pochodzą z pomiaru M3.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Polityka {
    /// Ile najlepszych tokenów verifiera uznajemy za „zgodne".
    pub k: usize,
    /// Maksymalny margines `logprob(top1) - logprob(node)`, gdy node jest w top-k.
    /// (Zmierzone 0,07–0,17 → domyślnie 0,25 daje zapas; top-1 vs top-2 to nie szum.)
    pub max_margin: f64,
    /// Dopuszczalny ułamek kroków podejrzanych (poza top-k albo poza marginesem).
    pub max_poza: f64,
}

impl Default for Polityka {
    fn default() -> Self {
        Self {
            k: 2,
            max_margin: 0.25,
            max_poza: 0.02,
        }
    }
}

/// Werdykt audytu. Wszystkie pola są deterministyczne i nadają się do logu/slashingu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Werdykt {
    pub krokow: usize,
    pub w_topk: usize,
    pub poza_topk: usize,
    /// Najgorszy zaobserwowany rank w top-k (0 = poza top-k).
    pub najgorszy_rank: usize,
    /// Największy margines logprob względem top-1.
    pub najgorszy_margin: f64,
    pub ok: bool,
    pub powod: Option<String>,
}

/// Audyt do weryfikacji (zgodny z JSON-em z verifiera).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Audyt {
    pub kroki: Vec<Krok>,
}

/// Ocenia kroki wobec polityki. Krok jest „podejrzany", gdy token node'a jest
/// poza top-k ALBO jest w top-k, ale margines do top-1 przekracza `max_margin`.
/// Werdykt `ok`, gdy ułamek kroków podejrzanych ≤ `max_poza` i audyt niepusty.
pub fn ocena(kroki: &[Krok], p: &Polityka) -> Werdykt {
    let mut w_topk = 0usize;
    let mut poza = 0usize;
    let mut najgorszy_rank = 0usize;
    let mut najgorszy_margin = 0.0f64;
    let mut powod: Option<String> = None;

    for (i, kr) in kroki.iter().enumerate() {
        let top1_lp = kr.topk.first().map(|x| x.1);
        let traf = kr
            .topk
            .iter()
            .enumerate()
            .find(|(_, (id, _))| *id == kr.node_token);

        match traf {
            Some((rank0, (_, lp))) => {
                let rank = rank0 + 1;
                let margin = match top1_lp {
                    Some(t1) => (t1 - *lp).max(0.0),
                    None => 0.0,
                };
                if rank > najgorszy_rank {
                    najgorszy_rank = rank;
                }
                if margin > najgorszy_margin {
                    najgorszy_margin = margin;
                }
                if rank <= p.k && margin <= p.max_margin {
                    w_topk += 1;
                } else {
                    poza += 1;
                    if powod.is_none() {
                        powod = Some(format!(
                            "krok {i} (idx {}): rank {rank} (k={}), margines {margin:.4} (max {:.4})",
                            kr.indeks, p.k, p.max_margin
                        ));
                    }
                }
            }
            None => {
                poza += 1;
                if powod.is_none() {
                    powod = Some(format!(
                        "krok {i} (idx {}): token {} POZA top-{}",
                        kr.indeks, kr.node_token, p.k
                    ));
                }
            }
        }
    }

    let ułamek = if kroki.is_empty() {
        1.0
    } else {
        poza as f64 / kroki.len() as f64
    };
    let ok = !kroki.is_empty() && ułamek <= p.max_poza;
    if ok {
        powod = None;
    }

    Werdykt {
        krokow: kroki.len(),
        w_topk,
        poza_topk: poza,
        najgorszy_rank,
        najgorszy_margin,
        ok,
        powod,
    }
}

/// Odcisk audytu — tylko liczby całkowite (indeksy + top-k token ids), bez floatów,
/// więc kanoniczny między implementacjami. Do przypięcia, CO dokładnie sprawdzono.
pub fn odcisk_audytu(kroki: &[Krok]) -> Result<String, SimonError> {
    let uproszczone: Vec<(u32, u32, Vec<u32>)> = kroki
        .iter()
        .map(|k| {
            (
                k.indeks,
                k.node_token,
                k.topk.iter().map(|(id, _)| *id).collect(),
            )
        })
        .collect();
    content_digest(&serde_json::json!({
        "domena": DOMENA_AUDYTU,
        "kroki": uproszczone,
    }))
}

/// Parsuje JSON audytu i od razu ocenia.
pub fn ocena_json(json: &str, p: &Polityka) -> Result<Werdykt, SimonError> {
    let a: Audyt = serde_json::from_str(json)?;
    Ok(ocena(&a.kroki, p))
}

/// Decyzja rozliczeniowa z audytu M3. **Kluczowa reguła** (E0/M3 kalibracja):
/// nigdy nie slasujemy za pojedynczy token ani za równość tokenów. Soft-fail to
/// eskalacja do człowieka/dalszego audytu BEZ slasha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decyzja {
    /// Audyt przeszedł — rozlicz normalnie.
    Pass,
    /// Dryf niejednoznaczny — eskalacja BEZ slasha.
    SoftFail,
    /// Wyraźne odejście — slash.
    HardFail,
    /// Brak podstaw: receipt poniżej poziomu 1 (brak wiązania wejścia) albo pusty audyt.
    BrakPodstaw,
}

/// Progi decyzji. `twardy_poza` = od jakiego ułamka kroków poza top-k zaczyna się slash.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Progi {
    pub twardy_poza: f64,
}

impl Default for Progi {
    fn default() -> Self {
        Self { twardy_poza: 0.05 }
    }
}

/// Zamienia werdykt M3 + poziom receiptu na decyzję rozliczeniową.
///
/// Bramki:
/// 1. `receipt_level < 1` → `BrakPodstaw` (nie ma wiązania wejścia, nie ma czego
///    audytować — dziś receipty są poziomu 0).
/// 2. puste kroki → `BrakPodstaw` (nie slasujemy na podstawie niczego).
/// 3. `ok` → `Pass`.
/// 4. ułamek kroków podejrzanych ≥ `twardy_poza` → `HardFail`, inaczej `SoftFail`.
///
/// „Podejrzany" = poza top-k albo w top-k z marginesem > `max_margin` (patrz `ocena`).
pub fn decyzja(w: &Werdykt, receipt_level: u8, progi: &Progi) -> Decyzja {
    if receipt_level < 1 || w.krokow == 0 {
        return Decyzja::BrakPodstaw;
    }
    if w.ok {
        return Decyzja::Pass;
    }
    let ułamek = w.poza_topk as f64 / w.krokow as f64;
    if ułamek >= progi.twardy_poza {
        Decyzja::HardFail
    } else {
        Decyzja::SoftFail
    }
}

#[cfg(test)]
mod testy {
    use super::*;

    fn topk(pary: &[(u32, f64)]) -> Vec<(u32, f64)> {
        pary.to_vec()
    }

    #[test]
    fn uczciwa_dywergencja_top1_top2_przechodzi() {
        // Realne z pomiaru: rank 1 lub 2, margines ≤ 0,17.
        let kroki = vec![
            Krok { indeks: 0, node_token: 528, topk: topk(&[(528, -1.20), (28742, -1.37)]) },
            Krok { indeks: 1, node_token: 302, topk: topk(&[(28725, -0.90), (302, -0.96)]) },
            Krok { indeks: 2, node_token: 28783, topk: topk(&[(28783, -2.10), (28774, -2.17)]) },
        ];
        let w = ocena(&kroki, &Polityka::default());
        assert!(w.ok, "{w:?}");
        assert_eq!(w.krokow, 3);
        assert_eq!(w.w_topk, 3);
        assert_eq!(w.poza_topk, 0);
        assert_eq!(w.najgorszy_rank, 2);
        assert!(w.najgorszy_margin <= 0.25 + 1e-9);
    }

    #[test]
    fn token_poza_topk_jest_odrzucony() {
        // Cheater: token daleko poza top-k.
        let kroki = vec![
            Krok { indeks: 0, node_token: 528, topk: topk(&[(528, -1.0)]) },
            Krok { indeks: 1, node_token: 999999, topk: topk(&[(28725, -0.9), (302, -0.96)]) },
        ];
        let w = ocena(&kroki, &Polityka::default());
        assert!(!w.ok, "{w:?}");
        assert_eq!(w.poza_topk, 1);
        assert!(w.powod.unwrap().contains("POZA top-2"));
    }

    #[test]
    fn wielki_margines_jest_podejrzany() {
        // Node w top-2, ale bardzo daleko od top-1 → podejrzane.
        let kroki = vec![Krok {
            indeks: 0,
            node_token: 302,
            topk: topk(&[(28725, -0.1), (302, -3.0)]),
        }];
        let w = ocena(&kroki, &Polityka::default());
        assert!(!w.ok, "{w:?}");
        assert!(w.najgorszy_margin > 2.0);
    }

    #[test]
    fn pusty_audyt_nie_przechodzi() {
        let w = ocena(&[], &Polityka::default());
        assert!(!w.ok);
        assert_eq!(w.krokow, 0);
    }

    #[test]
    fn decyzja_blokuje_slash_bez_poziomu_1_i_przy_pustym_audycie() {
        let w = ocena(&[], &Polityka::default());
        assert_eq!(decyzja(&w, 1, &Progi::default()), Decyzja::BrakPodstaw);
        let kroki = vec![Krok { indeks: 0, node_token: 1, topk: topk(&[(1, -0.1)]) }];
        let w2 = ocena(&kroki, &Polityka::default());
        assert_eq!(decyzja(&w2, 0, &Progi::default()), Decyzja::BrakPodstaw);
    }

    #[test]
    fn decyzja_soft_bez_slasha_przy_jednym_odstepstwie() {
        // 1 podejrzany krok na 30 (3,3%): > max_poza (2%) ale < twardy próg (5%)
        // → audyt NIE ok, ale decyzja SoftFail, NIE slash.
        let mut kroki: Vec<Krok> = (0..29)
            .map(|i| Krok { indeks: i, node_token: 1, topk: topk(&[(1, -0.1)]) })
            .collect();
        kroki.push(Krok { indeks: 29, node_token: 999, topk: topk(&[(1, -0.1), (2, -0.2)]) });
        let w = ocena(&kroki, &Polityka::default());
        assert!(!w.ok, "{w:?}");
        assert_eq!(decyzja(&w, 1, &Progi::default()), Decyzja::SoftFail);
    }

    #[test]
    fn decyzja_hard_przy_wielu_odstepstwach() {
        // 10/50 = 20% ≥ 5% → HardFail.
        let kroki: Vec<Krok> = (0..50)
            .map(|i| Krok {
                indeks: i,
                node_token: if i < 10 { 999 } else { 1 },
                topk: topk(&[(1, -0.1)]),
            })
            .collect();
        let w = ocena(&kroki, &Polityka::default());
        assert_eq!(decyzja(&w, 1, &Progi::default()), Decyzja::HardFail);
    }

    #[test]
    fn odcisk_audytu_jest_deterministyczny_i_czuly() {
        let a = vec![Krok { indeks: 0, node_token: 1, topk: topk(&[(1, -0.1), (2, -0.2)]) }];
        let b = a.clone();
        let mut c = a.clone();
        c[0].node_token = 3;
        assert_eq!(odcisk_audytu(&a).unwrap(), odcisk_audytu(&b).unwrap());
        assert_ne!(odcisk_audytu(&a).unwrap(), odcisk_audytu(&c).unwrap());
    }
}
