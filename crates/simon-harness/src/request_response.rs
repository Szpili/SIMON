//! M1.1 — kanał request-response dla promptu i wyniku (NIGDY gossipsub).
//!
//! **Dlaczego nie gossipsub:** gossipsub to rozgłaszanie do wszystkich
//! subskrybentów. Prompt wysłany tam **widzi cały swarm**, co łamie RULES #1
//! (i decyzję MVP: „jeden node widzi prompt"). Zlecenie idzie przez gossipsub
//! — to jest jawne. **Prompt i wynik idą kanałem punkt-punkt, między klientem
//! a konkretnym nodem.**
//!
//! Protokół:
//!   `/simon/prompt/1`  — klient → node: treść promptu, klient czeka na to samo połączenie
//!   `/simon/result/1`  — node → klient: wynik + receipt

use libp2p::request_response::{self, ProtocolSupport};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Zapytanie o policzenie promptu (klient → node).
///
/// **Uwaga:** tu JEST treść promptu. To jest świadomy koszt decyzji MVP —
/// jeden node widzi tekst, szyfrowany w drodze (patrz ROADMAP M1.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptRequest {
    /// M0.4a: wersja formatu.
    pub v: u16,
    pub order_id: String,
    pub job_id: String,
    /// Model, o który prosi klient (D11: user wybiera MODEL, nie maszynę).
    pub model_hash: String,
    /// Treść promptu — W KANALE PUNKT-PUNKT, nigdy w gossipsub.
    pub prompt: String,
    pub max_tokens: u32,
    /// C: ile tokenów kontekstu klient deklaruje, że wysyła.
    ///
    /// Node porównuje to ze swoim `max_ctx` (odczytanym z `/v1/models`
    /// → `max_model_len`). Brak dopasowania = kontrolowany `PromptError`,
    /// NIE błąd z vLLM. Bez tego za długie zlecenie wywalało się u node'a.
    pub max_ctx: u32,
    /// Nonce zlecenia (M0.2/M0.4b) — wiąże prompt z autoryzacją.
    pub nonce: String,
}

/// Odpowiedź node'a (node → klient).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptResponse {
    pub v: u16,
    pub order_id: String,
    pub job_id: String,
    /// Wynik inferencji. Wysyłany RAZEM z receiptem — klient weryfikuje SAM.
    pub output: String,
    /// Podpisany receipt (JSON) — klient sprawdza go lokalnie (RULES #6).
    pub receipt: String,
    /// Ile tokenów wygenerowano (do pomiaru kosztu, M2.7).
    pub tokens_out: u32,
    /// TTFT w ms (do pomiaru, M2.7).
    pub ttft_ms: u64,
    /// Czas generowania w ms (do pomiaru, M2.7).
    pub gen_ms: u64,
}

/// Błąd po stronie node'a (np. model zajęty, model uśpiony).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptError {
    pub v: u16,
    pub order_id: String,
    pub kod: String,
    pub opis: String,
    /// Przy `kod == "ctx_ponad_limit"`: REALNA liczba tokenów zlecenia
    /// (z `/tokenize` node'a, C+1) i limit node'a. Ustawiane strukturalnie,
    /// żeby wołający (np. map-reduce w agent.rs) nie musiał parsować `opis`
    /// — to jest dokładnie rodzina błędów "format wygląda tak samo"
    /// (ROADMAP.md), której unikamy: WOLNY tekst do czytania przez
    /// człowieka nie jest kontraktem programistycznym.
    pub realny_ctx: Option<u32>,
    pub limit_ctx: Option<u32>,
}

/// Podsumowanie odpowiedzi — albo wynik, albo błąd.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PromptReply {
    Ok(PromptResponse),
    Blad(PromptError),
}

/// Protokoły request-response (M1.1).
#[derive(Debug, Clone)]
pub struct ProtokolPromptu;

impl AsRef<str> for ProtokolPromptu {
    fn as_ref(&self) -> &str {
        "/simon/prompt/1"
    }
}

#[derive(Debug, Clone)]
pub struct ProtokolWyniku;

impl AsRef<str> for ProtokolWyniku {
    fn as_ref(&self) -> &str {
        "/simon/result/1"
    }
}

/// Domyślny `request_timeout` w `libp2p-request-response` to **10 sekund**
/// (`Config::default()`) — poziom PROTOKOŁU, niezależny od tego, jak długo
/// klient sam jest gotów czekać (agent.rs ma własne 300s). Przy dłuższym
/// TTFT (zmierzone na żywo: 10,6-11,0s dla qwen3.8-27b pod obciążeniem,
/// 2026-09-17, test map-reduce na Szponie) node liczy odpowiedź poprawnie,
/// ale `send_response` pada, bo libp2p już ubił kanał po swoich 10s —
/// wygląda jak zwis sieci, a to timeout niżej niż ktokolwiek ustawiał
/// świadomie. Podnosimy do wartości pokrywającej realny czas generowania.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// Tworzy behaviour request-response dla promptu (klient ↔ node).
pub fn behaviour() -> request_response::cbor::Behaviour<PromptRequest, PromptReply> {
    request_response::cbor::Behaviour::new(
        [(
            libp2p::StreamProtocol::new(ProtokolPromptu.as_ref()),
            ProtocolSupport::Full,
        )],
        request_response::Config::default().with_request_timeout(REQUEST_TIMEOUT),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m11_prompt_nie_jest_w_gossipsub() {
        // Protokół promptu JEST osobny od tematów gossipsub.
        assert!(ProtokolPromptu.as_ref().starts_with("/simon/prompt/"));
        assert!(!ProtokolPromptu.as_ref().contains("gossipsub"));
        assert_ne!(ProtokolPromptu.as_ref(), ProtokolWyniku.as_ref());
    }

    #[test]
    fn m11_roundtrip_promptu() {
        let req = PromptRequest {
            v: 1,
            order_id: "abc".into(),
            job_id: "job1".into(),
            model_hash: "sha256:qwen".into(),
            prompt: "policz to".into(),
            max_tokens: 128,
            max_ctx: 1000,
            nonce: "n1".into(),
        };
        let bajty = serde_json::to_vec(&req).unwrap();
        let back: PromptRequest = serde_json::from_slice(&bajty).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn m11_roundtrip_wyniku() {
        let r = PromptReply::Ok(PromptResponse {
            v: 1,
            order_id: "abc".into(),
            job_id: "job1".into(),
            output: "wynik".into(),
            receipt: "{}".into(),
            tokens_out: 5,
            ttft_ms: 1740,
            gen_ms: 230,
        });
        let back: PromptReply = serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
        assert_eq!(r, back);
    }
}
