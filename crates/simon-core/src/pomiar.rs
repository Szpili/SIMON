//! Wykrywanie „pomiarów", które są w rzeczywistości wpisaną stałą.
//!
//! Powód istnienia tego modułu (incydent 2026-09-18): `gen_ms` liczono jako
//! `tokens_out / 65.0 * 1000` — ze stałej zmierzonej kiedyś dla jednego modelu
//! na jednej karcie, stosowanej do wszystkich węzłów. Skutek: przepustowość
//! wychodziła ZAWSZE 65,0 tok/s. Błąd przeszedł przez kod, przez komentarz
//! twierdzący „twarda miara, nie heurystyka tok/s" i przez demo pokazujące tę
//! samą liczbę dla dwóch różnych kart — sygnał był widoczny i został
//! przeoczony kilka razy.
//!
//! Wniosek: brakowało nie tylko pomiaru, ale **testu wykrywającego podejrzanie
//! stałą wartość**. Dwa różne profile wykonania (inny model albo inna karta)
//! nie mogą dać identycznego wyniku do wielu miejsc po przecinku.

/// Czy dwie wartości pochodzące z RÓŻNYCH profili wykonania są podejrzanie
/// identyczne. `tolerancja` to dopuszczalna różnica względna.
///
/// To jest heurystyka alarmowa, nie dowód: dwa profile mogą przypadkiem wypaść
/// blisko siebie. Ale identyczność do trzeciego miejsca po przecinku przy
/// różnym sprzęcie oznacza zwykle, że liczba nie pochodzi z pomiaru.
pub fn podejrzanie_identyczna(a: f64, b: f64, tolerancja: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let skala = a.abs().max(b.abs());
    if skala == 0.0 {
        // Dwa zera: albo nic nie policzono, albo licznik jest martwy.
        // Nie alarmujemy — to łapią inne bramki.
        return false;
    }
    (a - b).abs() / skala < tolerancja
}

/// Domyślna tolerancja dla porównania profili: 0,1%.
pub const TOLERANCJA_PROFILI: f64 = 0.001;

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn stala_65_z_incydentu_zostalaby_zlapana() {
        // Dokładnie to, co pokazywało demo przed poprawką: vLLM na 3090
        // i Ollama na 3080 Ti, obie „65,0 tok/s".
        assert!(
            podejrzanie_identyczna(65.0, 65.0, TOLERANCJA_PROFILI),
            "identyczna przepustowosc dla dwoch roznych profili to alarm"
        );
    }

    #[test]
    fn realne_pomiary_po_poprawce_przechodza() {
        // Zmierzone 2026-09-18 po naprawie: 40 tok w 910 ms vs 26 tok w 7798 ms.
        let vllm = 40.0 / 0.910;
        let ollama = 26.0 / 7.798;
        assert!(!podejrzanie_identyczna(vllm, ollama, TOLERANCJA_PROFILI));
    }

    #[test]
    fn bliskie_ale_nie_identyczne_nie_alarmuje() {
        assert!(!podejrzanie_identyczna(64.0, 65.0, TOLERANCJA_PROFILI));
    }

    #[test]
    fn zera_nie_alarmuja() {
        // Brak wygenerowanych tokenów to inny problem i inna bramka.
        assert!(!podejrzanie_identyczna(0.0, 0.0, TOLERANCJA_PROFILI));
    }
}
