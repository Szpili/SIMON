//! I5 (M7) — limity zasobów na granicy node. **Deterministyczna porażka**, nie „best effort".
//!
//! Krytyk: przy P2P node, który podpisuje wiarygodne receipty, ale zjada nieograniczony CPU,
//! jest problemem DoS. To jest twarda bramka: deadline, budżet tokenów, budżet bajtów, limit
//! kroków narzędzi, anulowanie. Każde przekroczenie zwraca jawny błąd; anulowanie wygrywa.

use serde::{Deserialize, Serialize};

/// Limity dla jednego zlecenia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limity {
    /// Budżet czasu od startu (mikrosekundy, zegar MONOTONICZNY).
    pub deadline_us: u64,
    pub max_tokenow: u32,
    pub max_bajtow: usize,
    pub max_krokow_narzedzi: u32,
}

impl Default for Limity {
    fn default() -> Self {
        Self {
            deadline_us: 120_000_000, // 120 s
            max_tokenow: 4096,
            max_bajtow: 8 * 1024 * 1024,
            max_krokow_narzedzi: 16,
        }
    }
}

/// Powód przerwania (deterministyczny).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Przekroczenie {
    Deadline,
    Tokeny,
    Bajty,
    KrokiNarzedzi,
    Anulowane,
}

/// Bieżące zużycie wobec limitów. Jedna instancja = jedno zlecenie.
#[derive(Debug, Clone)]
pub struct Budzet {
    limity: Limity,
    start_us: u64,
    tokeny: u32,
    bajty: usize,
    kroki: u32,
    anulowane: bool,
}

impl Budzet {
    pub fn nowy(limity: Limity, teraz_us: u64) -> Self {
        Self {
            limity,
            start_us: teraz_us,
            tokeny: 0,
            bajty: 0,
            kroki: 0,
            anulowane: false,
        }
    }

    /// Anulowanie (np. klient się rozłączył) — wygrywa z resztą.
    pub fn anuluj(&mut self) {
        self.anulowane = true;
    }

    /// Bramka czasu/kolejności. Wołana przed każdym krokiem.
    pub fn sprawdz(&self, teraz_us: u64) -> Result<(), Przekroczenie> {
        if self.anulowane {
            return Err(Przekroczenie::Anulowane);
        }
        if teraz_us.saturating_sub(self.start_us) > self.limity.deadline_us {
            return Err(Przekroczenie::Deadline);
        }
        Ok(())
    }

    pub fn dodaj_tokeny(&mut self, n: u32, teraz_us: u64) -> Result<(), Przekroczenie> {
        self.sprawdz(teraz_us)?;
        self.tokeny = self.tokeny.saturating_add(n);
        if self.tokeny > self.limity.max_tokenow {
            return Err(Przekroczenie::Tokeny);
        }
        Ok(())
    }

    pub fn dodaj_bajty(&mut self, n: usize, teraz_us: u64) -> Result<(), Przekroczenie> {
        self.sprawdz(teraz_us)?;
        self.bajty = self.bajty.saturating_add(n);
        if self.bajty > self.limity.max_bajtow {
            return Err(Przekroczenie::Bajty);
        }
        Ok(())
    }

    pub fn krok_narzedzia(&mut self, teraz_us: u64) -> Result<(), Przekroczenie> {
        self.sprawdz(teraz_us)?;
        self.kroki = self.kroki.saturating_add(1);
        if self.kroki > self.limity.max_krokow_narzedzi {
            return Err(Przekroczenie::KrokiNarzedzi);
        }
        Ok(())
    }

    pub fn zuzycie(&self) -> (u32, usize, u32) {
        (self.tokeny, self.bajty, self.kroki)
    }
}

#[cfg(test)]
mod testy {
    use super::*;

    fn lim() -> Limity {
        Limity {
            deadline_us: 1_000_000,
            max_tokenow: 10,
            max_bajtow: 100,
            max_krokow_narzedzi: 2,
        }
    }

    #[test]
    fn deadline_przerywa_deterministycznie() {
        let mut b = Budzet::nowy(lim(), 0);
        assert!(b.sprawdz(500_000).is_ok());
        assert_eq!(b.sprawdz(1_000_001), Err(Przekroczenie::Deadline));
        assert_eq!(b.dodaj_tokeny(1, 2_000_000), Err(Przekroczenie::Deadline));
    }

    #[test]
    fn budzety_tokenow_bajtow_krokow_lamia_sie_na_limicie() {
        let mut b = Budzet::nowy(lim(), 0);
        assert!(b.dodaj_tokeny(10, 0).is_ok());
        assert_eq!(b.dodaj_tokeny(1, 0), Err(Przekroczenie::Tokeny));

        let mut b = Budzet::nowy(lim(), 0);
        assert!(b.dodaj_bajty(100, 0).is_ok());
        assert_eq!(b.dodaj_bajty(1, 0), Err(Przekroczenie::Bajty));

        let mut b = Budzet::nowy(lim(), 0);
        assert!(b.krok_narzedzia(0).is_ok());
        assert!(b.krok_narzedzia(0).is_ok());
        assert_eq!(b.krok_narzedzia(0), Err(Przekroczenie::KrokiNarzedzi));
    }

    #[test]
    fn anulowanie_wygrywa_z_reszta() {
        let mut b = Budzet::nowy(lim(), 0);
        b.anuluj();
        assert_eq!(b.sprawdz(0), Err(Przekroczenie::Anulowane));
        assert_eq!(b.dodaj_tokeny(1, 0), Err(Przekroczenie::Anulowane));
        assert_eq!(b.krok_narzedzia(0), Err(Przekroczenie::Anulowane));
        assert_eq!(b.zuzycie(), (0, 0, 0));
    }
}
