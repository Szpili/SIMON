//! I4 (M7) — etykiety zaufania i przepływy (IFC, zawężone).
//!
//! Krytyk (bunny): „pełne IFC" to aspiracja; zacznij od małego, konkretnego zbioru
//! reguł. To jest ten zbiór: **deny-by-default** na przepływach do „sinków", z jawną
//! deklasacją operatora. Nie twierdzimy, że to rozwiązuje prompt injection — to
//! ogranicza **uprawnienia** danych, które już weszły do systemu.
//!
//! Sedno: treść `retrieved`/`peer`/`tool`/`model` **nigdy** nie staje się instrukcją
//! ani argumentem akcji bez deklasacji. Tylko `operator` pisze do sinków wprost.

use serde::{Deserialize, Serialize};

/// Skąd pochodzi dana wartość/treść.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zaufanie {
    /// Operator (człowiek/właściciel maszyny) — jedyne źródło zaufane.
    Operator,
    /// Wejście klienta (zlecenie) — zaufane w zakresie intencji, nie treści.
    ClientInput,
    /// Tekst zwrócony przez inny node (wynik A w łańcuchu A→B).
    PeerText,
    /// Treść pobrana (RAG, dokument, web).
    RetrievedText,
    /// Wynik narzędzia (read_file, search_web, …).
    ToolText,
    /// Tekst wygenerowany przez model (output).
    ModelDerived,
}

/// Dokąd wartość może popłynąć.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sink {
    /// Argument akcji/narzędzia (efekt zewnętrzny).
    ArgumentAkcji,
    /// Instrukcje kolejnego zlecenia (łańcuch A→B).
    InstrukcjaKolejnegoZlecenia,
    /// System prompt.
    SystemPrompt,
    /// Log/audyt (append-only; nie steruje akcją).
    Log,
}

/// Czy przepływ `zrodlo -> sink` jest dozwolony **bez** deklasacji operatora.
/// Deny-by-default: jedynie `Operator` pisze do sinków sterujących, log przyjmuje wszystko.
pub fn dozwolony_bez_deklasacji(zrodlo: Zaufanie, sink: Sink) -> bool {
    use Sink::*;
    use Zaufanie::*;
    match sink {
        Log => true,
        ArgumentAkcji | InstrukcjaKolejnegoZlecenia | SystemPrompt => matches!(zrodlo, Operator),
    }
}

/// Czy przepływ wymaga jawnej deklasacji operatora (dla par, które nie są wprost dozwolone).
pub fn wymaga_deklasacji(zrodlo: Zaufanie, sink: Sink) -> bool {
    !dozwolony_bez_deklasacji(zrodlo, sink)
}

/// Decyzja o przepływie: dozwolony bez deklasacji, albo obecny w jawnie zatwierdzonych
/// deklasacjach `(zrodlo, sink)`. Rejestr deklasacji wystawia operator (audytowalnie).
pub fn przeplyw_ok(zrodlo: Zaufanie, sink: Sink, deklasacje: &[(Zaufanie, Sink)]) -> bool {
    dozwolony_bez_deklasacji(zrodlo, sink)
        || deklasacje
            .iter()
            .any(|(z, s)| *z == zrodlo && *s == sink)
}

#[cfg(test)]
mod testy {
    use super::*;
    use Sink::*;
    use Zaufanie::*;

    #[test]
    fn domyslnie_odrzuca_sa_niezaufane_do_sinkow() {
        // To jest sedno I4: te przepływy muszą być domyślnie zabronione.
        assert!(!przeplyw_ok(PeerText, InstrukcjaKolejnegoZlecenia, &[]));
        assert!(!przeplyw_ok(RetrievedText, ArgumentAkcji, &[]));
        assert!(!przeplyw_ok(ToolText, ArgumentAkcji, &[]));
        assert!(!przeplyw_ok(ModelDerived, ArgumentAkcji, &[]));
        assert!(!przeplyw_ok(RetrievedText, SystemPrompt, &[]));
        assert!(!przeplyw_ok(PeerText, SystemPrompt, &[]));
    }

    #[test]
    fn operator_pisze_do_wszystkich_sinkow_a_log_przyjmuje_wszystko() {
        for s in [ArgumentAkcji, InstrukcjaKolejnegoZlecenia, SystemPrompt, Log] {
            assert!(przeplyw_ok(Operator, s, &[]), "operator -> {s:?}");
        }
        for z in [Operator, ClientInput, PeerText, RetrievedText, ToolText, ModelDerived] {
            assert!(przeplyw_ok(z, Log, &[]), "{z:?} -> Log");
        }
    }

    #[test]
    fn jawna_deklasacja_operatora_otwiera_konkretny_przeplyw() {
        // Tylko dokładnie ta para; sąsiednie zostają zabronione.
        let dekl = vec![(RetrievedText, ArgumentAkcji)];
        assert!(przeplyw_ok(RetrievedText, ArgumentAkcji, &dekl));
        assert!(!przeplyw_ok(PeerText, ArgumentAkcji, &dekl));
        assert!(!przeplyw_ok(RetrievedText, SystemPrompt, &dekl));
        assert!(wymaga_deklasacji(RetrievedText, ArgumentAkcji));
        assert!(!wymaga_deklasacji(Operator, ArgumentAkcji));
    }
}
