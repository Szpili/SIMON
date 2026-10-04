//! #4 (M7) — hardening protokołu: **replay (nonce)**, **rate-limit**, **limit rozmiaru
//! wiadomości**, propagacja anulowania. Deterministyczne, bez sieci — testowalne na CPU.
//!
//! Kontekst (krytyk): P2P bez TEE — trzeba jawnie odciąć powtarzalne/stare nonce,
//! zalać wiadomościami i zbyt duże ramki, zanim dotrą do modelu. Anulowanie ma
//! wygrywać (patrz `limity::Budzet::anuluj`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maksymalny rozmiar jednej wiadomości protokołu (bajty).
pub const MAX_WIADOMOSC_BAJTOW: usize = 1 << 20; // 1 MiB

/// Błąd protokołu (deterministyczny powód odrzucenia).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BladProtokolu {
    NoncePowtorzony,
    NonceZaStary,
    WiadomoscZaDuza,
    RateLimit,
}

/// Okno anty-replay. Akceptuje nonce w oknie `(max - okno, …]`, każdy **tylko raz**.
#[derive(Debug, Clone)]
pub struct OknoNonce {
    max: Option<u64>,
    okno: u64,
    widziane: BTreeSet<u64>,
}

impl OknoNonce {
    pub fn nowy(okno: u64) -> Self {
        Self {
            max: None,
            okno,
            widziane: BTreeSet::new(),
        }
    }

    /// Przyjmuje nonce albo zwraca powód odrzucenia.
    pub fn przyjmij(&mut self, nonce: u64) -> Result<(), BladProtokolu> {
        if let Some(m) = self.max {
            if nonce <= m.saturating_sub(self.okno) {
                return Err(BladProtokolu::NonceZaStary);
            }
        }
        if !self.widziane.insert(nonce) {
            return Err(BladProtokolu::NoncePowtorzony);
        }
        if self.max.map(|m| nonce > m).unwrap_or(true) {
            self.max = Some(nonce);
        }
        if let Some(m) = self.max {
            let granica = m.saturating_sub(self.okno);
            self.widziane.retain(|n| *n >= granica);
        }
        Ok(())
    }
}

/// Limit rozmiaru wiadomości.
pub fn sprawdz_rozmiar(n: usize) -> Result<(), BladProtokolu> {
    if n > MAX_WIADOMOSC_BAJTOW {
        Err(BladProtokolu::WiadomoscZaDuza)
    } else {
        Ok(())
    }
}

/// Token bucket: `pojemnosc` = burst, `tempo` = tokeny/s. Jeden kubełek = jeden peer.
#[derive(Debug, Clone)]
pub struct Kubelek {
    pojemnosc: f64,
    tempo: f64,
    dostepne: f64,
    ostatni_us: u64,
}

impl Kubelek {
    pub fn nowy(pojemnosc: f64, tempo_na_s: f64, teraz_us: u64) -> Self {
        Self {
            pojemnosc,
            tempo: tempo_na_s,
            dostepne: pojemnosc,
            ostatni_us: teraz_us,
        }
    }

    /// Zezwala na jedną wiadomość albo zwraca `RateLimit`.
    pub fn zezwol(&mut self, teraz_us: u64) -> Result<(), BladProtokolu> {
        let dt = teraz_us.saturating_sub(self.ostatni_us) as f64 / 1_000_000.0;
        self.ostatni_us = teraz_us;
        self.dostepne = (self.dostepne + dt * self.tempo).min(self.pojemnosc);
        if self.dostepne >= 1.0 {
            self.dostepne -= 1.0;
            Ok(())
        } else {
            Err(BladProtokolu::RateLimit)
        }
    }
}

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn nonce_powtorzony_i_stary_odrzucone() {
        let mut o = OknoNonce::nowy(10);
        assert!(o.przyjmij(100).is_ok());
        assert_eq!(o.przyjmij(100), Err(BladProtokolu::NoncePowtorzony));
        assert!(o.przyjmij(105).is_ok());
        assert!(o.przyjmij(103).is_ok()); // w oknie, jeszcze nie widziany
        assert_eq!(o.przyjmij(103), Err(BladProtokolu::NoncePowtorzony));
        // 100 wypada z okna (105-10=95 → nie; 106 ustawia max=106, granica 96 → 100 nadal w oknie)
        assert!(o.przyjmij(120).is_ok());
        // teraz granica = 110; 100 jest poza oknem → "za stary" (a nie "powtórzony")
        assert_eq!(o.przyjmij(100), Err(BladProtokolu::NonceZaStary));
    }

    #[test]
    fn rozmiar_wiadomosci_odrzuca_za_duze() {
        assert!(sprawdz_rozmiar(MAX_WIADOMOSC_BAJTOW).is_ok());
        assert_eq!(
            sprawdz_rozmiar(MAX_WIADOMOSC_BAJTOW + 1),
            Err(BladProtokolu::WiadomoscZaDuza)
        );
    }

    #[test]
    fn kubelek_dopuszcza_burst_potem_odrzuca_i_uzupelnia() {
        // burst 3, tempo 1/s
        let mut k = Kubelek::nowy(3.0, 1.0, 0);
        assert!(k.zezwol(0).is_ok());
        assert!(k.zezwol(0).is_ok());
        assert!(k.zezwol(0).is_ok());
        assert_eq!(k.zezwol(0), Err(BladProtokolu::RateLimit));
        // po 1 s wraca 1 token
        assert!(k.zezwol(1_000_000).is_ok());
        assert_eq!(k.zezwol(1_000_000), Err(BladProtokolu::RateLimit));
    }
}
