//! Receipt wykonania — format ZAMROŻONY (odwracalność: wysoka).
//!
//! Naprawa F3 (red-team RT1, `~/DC`): „complete() nie waliduje podpisu"
//! → 100% rozliczeń do podrobienia, wykrywalność 0%. Łamało D12 i D18.
//!
//! Trzy bramki weryfikacji (wariant C):
//!   1. podpis ważny pod kluczem z receiptu,
//!   2. klucz z receiptu == klucz ZAREJESTROWANY dla tego node'a,
//!   3. receipt dotyczy TEGO joba i TEGO node'a (zgodność pól).
//!
//! Bez bramki (2) atakujący podpisałby własnym kluczem i wstawił swój pubkey —
//! podpis byłby „ważny", ale nie byłby dowodem na zarejestrowanego node'a.

use serde::{Deserialize, Serialize};

use crate::crypto::{Keypair, PublicKey, Signature};
use crate::{content_digest, SimonError};

/// Precyzja obliczeń (D67 pkt 1: jedna kwantyzacja per klaster).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Bf16,
    Fp16,
    Fp32,
    Fp8,
    Fp4,
}

/// Receipt jest tym, co node składa jako dowód wykonania pracy.
///
/// Pole `signature` NIE wchodzi do odcisku treści — podpis podpisuje resztę.
/// Pole `signer` JEST częścią treści (mówi KTO podpisał) i musi zgadzać się
/// z kluczem zarejestrowanym w rejestrze koordynatora.
/// Wiązanie M1: wejście (tokenizer, nonce, tokeny promptu) + stan (parametry
/// próbkowania, ziarno) + odcisk wyjścia po tokenach. JEDNO opcjonalne pole
/// w receipcie — stare literały i podpisy się nie psują.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WiazaniaM1 {
    pub schema_version: u32,
    pub receipt_level: u8,
    /// Profil wykonania dla audytu (np. "llama.cpp/Q4_K_M/cpu"). Bez niego
    /// re-run na innym sprzęcie nie ma szans się zgodzić (patrz E0).
    pub exec_profile: String,
    pub tokenizer_hash: String,
    pub prompt_digest: String,
    pub output_token_chain: String,
    pub client_nonce: String,
    pub sampling_params_hash: String,
    pub rng_seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub job_id: String,
    pub node_id: String,
    /// Hash modelu (D67 pkt 1: niezgodny = inny klaster).
    pub model_hash: String,
    /// Wersja runtime (D67 pkt 2: niezgodna = nie wchodzi do puli).
    pub runtime: String,
    pub precision: Precision,
    /// LSH aktywacji wg TopLoc (258 B / 32 tokeny, arXiv 2501.16007).
    pub activation_hash: String,
    /// Odcisk TREŚCI wyniku, związany z `job_id`.
    ///
    /// NAPRAWA (2026-09-18): wcześniej hashowano tu METADANE
    /// (`{job_id, tokens_out, ttft_ms, gen_ms}`), a sama odpowiedź szła obok
    /// receiptu NIEPODPISANA. Węzeł mógł odesłać dowolny tekst i receipt dalej
    /// przechodził weryfikację — czyli podpis dowodził „wykonałem jakąś pracę
    /// o tym id", a nie „to jest jej wynik". Teraz odcisk pokrywa treść,
    /// a klient MUSI go przeliczyć z tego, co dostał (patrz `zgodny_z_wyjsciem`).
    pub output_digest: String,
    /// Ile pracy NAPRAWDĘ wykonano — podpisane, bo na tym opiera się rozliczenie.
    /// Rozdzielone, bo prefill i decode kosztują skrajnie różnie (zmierzone:
    /// dekodowanie ~55x wolniejsze na token), więc jedna liczba „tokenów"
    /// byłaby zaproszeniem do arbitrażu.
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    /// Czas startu w MIKROSEKUNDACH (u64).
    ///
    /// NAPRAWA (2026-09-17): wcześniej `f64`. Float nie ma gwarantowanego
    /// roundtripu między formatami — JSON i CBOR serializują go inaczej,
    /// więc SHA-256 z treści wychodził INNY po przejściu przez transport
    /// i podpis przestawał pasować (`Err(BadSignature)`).
    /// Podpisywana treść NIE MOŻE zawierać floatów.
    pub started_at_us: u64,
    pub finished_at_us: u64,
    /// Klucz publiczny node'a (część treści — podlega podpisowi).
    pub signer: PublicKey,
    /// Podpis nad odciskiem treści. Wyłączony z odcisku.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<Signature>,

    /// M1: opcjonalne wiązanie wejścia/stanu. `None` = receipt legacy —
    /// serializuje się wtedy bajt w bajt jak przed M1, więc stare podpisy
    /// i fixtures pozostają ważne.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub wiazania: Option<WiazaniaM1>,
}

/// Separator domeny. Bez niego ten sam hash mogłby zostać podstawiony
/// w innym miejscu protokołu jako co innego.
pub const DOMENA_ODCISKU_WYJSCIA: &str = "SIMON/OUTPUT/v1";

// --- M1 (wiązanie wejścia i stanu) — separatory domen ---------------------
/// Domeny M1. Każda z osobna, żeby ten sam bajt-strumień nie mógł znaczyć
/// czego innego w innym miejscu protokołu.
pub const DOMENA_PROMPTU: &str = "SIMON/PROMPT/v1";
pub const DOMENA_LANCUCHA: &str = "SIMON/CHAIN/v1";
pub const DOMENA_PARAMETROW: &str = "SIMON/SAMPLING/v1";
pub const DOMENA_TOKENIZERA: &str = "SIMON/TOKENIZER/v1";

/// Wersja schematu receiptu. `None` = legacy (v0). Nowe pola są opcjonalne,
/// więc receipt bez nich serializuje się DOKŁADNIE tak jak przed M1 — stare
/// podpisy i `przyklady/receipt.json` pozostają ważne.
pub const SCHEMA_VERSION_M1: u32 = 2;

/// Poziom dowodu: 0 = tylko podpisany output; 1 = +wiązanie wejścia/stanu (M1).
pub const RECEIPT_LEVEL_M1: u8 = 1;

/// Odcisk treści wyniku, związany z konkretnym zleceniem — żeby poprawny
/// receipt z INNEGO zlecenia nie dał się podstawić pod ten sam tekst.
///
/// **Czego to NIE obejmuje (świadomy dług, nie przeoczenie):** commitment jest
/// po ZDEKODOWANYM TEKŚCIE, a nie po sekwencji `token_ids`. Różne sekwencje
/// tokenów mogą zdekodować się do tego samego tekstu, a weryfikacja inferencji
/// dotyczy sekwencji, którą model faktycznie przetworzył. Docelowy commitment
/// ma objąć `ordered_output_token_ids`, `finish_reason`, tool calle wraz
/// z argumentami, manifest modelu i tokenizera oraz prompt commitment —
/// patrz M5.2 w ROADMAP. Dziś żaden z naszych backendów nie wystawia
/// `token_ids` przez API OpenAI, więc byłby to commitment do czegoś,
/// czego klient i tak nie może sprawdzić.
pub fn odcisk_wyjscia(job_id: &str, output: &str) -> Result<String, SimonError> {
    content_digest(&serde_json::json!({
        "domena": DOMENA_ODCISKU_WYJSCIA,
        "job_id": job_id,
        "output": output,
    }))
}

/// Odcisk WEJŚCIA: tokenizer + client nonce + dokładne `prompt_token_ids`.
///
/// Wiąże to, co klient naprawdę wysłał, z tym, co node twierdzi, że przetworzył
/// — zamyka truncację promptu, zły tokenizer i re-templating. Kanonizacja przez
/// `content_digest` (tablica u32 w JSON, bez floatów) — spójna z `odcisk_wyjscia`.
pub fn odcisk_promptu(
    tokenizer_hash: &str,
    client_nonce: &str,
    prompt_token_ids: &[u32],
) -> Result<String, SimonError> {
    content_digest(&serde_json::json!({
        "domena": DOMENA_PROMPTU,
        "tokenizer_hash": tokenizer_hash,
        "nonce": client_nonce,
        "tokens": prompt_token_ids,
    }))
}

/// Odcisk WYJŚCIA po tokenach, związany z odciskiem promptu (M1).
/// Commit na sekwencji tokenów, nie na tekście — bo różne sekwencje mogą
/// zdekodować się do tego samego tekstu, a audyt dotyczy sekwencji.
pub fn lancuch_tokenow(
    prompt_digest: &str,
    output_token_ids: &[u32],
) -> Result<String, SimonError> {
    content_digest(&serde_json::json!({
        "domena": DOMENA_LANCUCHA,
        "prompt": prompt_digest,
        "tokens": output_token_ids,
    }))
}

/// Odcisk parametrów próbkowania. Wartości jako liczby całkowite (\*_milli),
/// bo podpisywana treść NIE MOŻE zawierać floatów (patrz NAPRAWA 2026-09-17).
/// Bez tego audyt re-runem nie ma stanu startowego.
pub fn odcisk_parametrow(
    temperature_milli: i64,
    top_p_milli: i64,
    top_k: u32,
    min_p_milli: i64,
    repeat_penalty_milli: i64,
) -> Result<String, SimonError> {
    content_digest(&serde_json::json!({
        "domena": DOMENA_PARAMETROW,
        "temperature_milli": temperature_milli,
        "top_p_milli": top_p_milli,
        "top_k": top_k,
        "min_p_milli": min_p_milli,
        "repeat_penalty_milli": repeat_penalty_milli,
    }))
}

/// Kanoniczny commitment parametrów próbkowania (krytyk bunny, 2026-10-04):
/// hash JSON-a użytkownika to za mało. Bindujemy **strukturę**, bez floatów
/// (liczby całkowite ×1e6).
///
/// **Czego to NIE jest:** obietnicą, że inny silnik/sprzęt odtworzy ten sam
/// strumień RNG. `rng_seed` + ten odcisk opisują **deklarowany stan wejściowy**,
/// a nie gwarancję reprodukcji (llama.cpp vs vLLM różnią się samplerem, batchowaniem
/// i kernelami). `exec_profile` pozostaje **twierdzeniem nieufnym**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplingCommitment {
    pub temperature_x1e6: u32,
    pub top_k: u16,
    pub top_p_x1e6: u32,
    pub min_p_x1e6: u32,
    pub typical_p_x1e6: u32,
    pub repeat_penalty_x1e6: u32,
    pub frequency_penalty_x1e6: u32,
    pub presence_penalty_x1e6: u32,
    pub mirostat_version: u8,
    pub mirostat_tau_x1e6: u32,
    pub mirostat_eta_x1e6: u32,
}

impl SamplingCommitment {
    /// Kanoniczny odcisk (domena `SIMON/SAMPLING/v1`), bez floatów.
    pub fn odcisk(&self) -> Result<String, SimonError> {
        content_digest(&serde_json::json!({
            "domena": DOMENA_PARAMETROW,
            "commitment": self,
        }))
    }

    /// Typowy greedy (temp=0, top_k=1) — stan z pomiarów E0/M3.
    pub fn greedy() -> Self {
        Self {
            temperature_x1e6: 0,
            top_k: 1,
            top_p_x1e6: 1_000_000,
            min_p_x1e6: 0,
            typical_p_x1e6: 1_000_000,
            repeat_penalty_x1e6: 1_000_000,
            frequency_penalty_x1e6: 0,
            presence_penalty_x1e6: 0,
            mirostat_version: 0,
            mirostat_tau_x1e6: 5_000_000,
            mirostat_eta_x1e6: 100_000,
        }
    }
}

/// Odcisk pliku tokenizera (`tokenizer.json`) — część manifestu modelu.
pub fn odcisk_tokenizera(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(DOMENA_TOKENIZERA.as_bytes());
    h.update([0u8]); // separator — odciski nie mogą się sklejać
    h.update(bytes);
    hex::encode(h.finalize())
}

impl Receipt {
    /// Czy ten receipt opisuje TEN tekst. Bez tej bramki podpis dowodzi tylko,
    /// że node coś policzył — nie, że to jest to, co trzymasz w ręku.
    pub fn zgodny_z_wyjsciem(&self, output: &str) -> bool {
        match odcisk_wyjscia(&self.job_id, output) {
            Ok(d) => d == self.output_digest,
            Err(_) => false,
        }
    }

    /// Poziom dowodu tego receiptu (0 = legacy, 1 = M1).
    pub fn poziom(&self) -> u8 {
        self.wiazania.as_ref().map(|w| w.receipt_level).unwrap_or(0)
    }

    /// Ustawia pola M1 (budowniczy dla node'a i testów).
    pub fn z_wiazaniem_m1(
        mut self,
        tokenizer_hash: String,
        client_nonce: String,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
        sampling_params_hash: String,
        rng_seed: u64,
        exec_profile: String,
    ) -> Result<Self, SimonError> {
        let pd = odcisk_promptu(&tokenizer_hash, &client_nonce, prompt_token_ids)?;
        let lc = lancuch_tokenow(&pd, output_token_ids)?;
        self.wiazania = Some(WiazaniaM1 {
            schema_version: SCHEMA_VERSION_M1,
            receipt_level: RECEIPT_LEVEL_M1,
            exec_profile,
            tokenizer_hash,
            prompt_digest: pd,
            output_token_chain: lc,
            client_nonce,
            sampling_params_hash,
            rng_seed,
        });
        Ok(self)
    }

    /// Bramka M1: czy receipt wiąże DOKŁADNIE ten tokenizer, nonce, prompt i
    /// wyjście (po tokenach). Weryfikacja CPU — bez wag i bez GPU.
    pub fn zweryfikuj_m1(
        &self,
        tokenizer_hash: &str,
        client_nonce: &str,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
    ) -> Result<(), SimonError> {
        let w = self.wiazania.as_ref().ok_or_else(|| {
            SimonError::ReceiptMismatch("brak wiązań M1 (poziom 0)".into())
        })?;
        if w.tokenizer_hash != tokenizer_hash {
            return Err(SimonError::ReceiptMismatch(
                "tokenizer_hash niezgodny z receiptem".into(),
            ));
        }
        if w.client_nonce != client_nonce {
            return Err(SimonError::ReceiptMismatch("client_nonce niezgodny".into()));
        }
        let oczekiwany_pd = odcisk_promptu(tokenizer_hash, client_nonce, prompt_token_ids)?;
        if w.prompt_digest != oczekiwany_pd {
            return Err(SimonError::ReceiptMismatch(
                "prompt_digest niezgodny — to nie ten prompt".into(),
            ));
        }
        let oczekiwany_lc = lancuch_tokenow(&w.prompt_digest, output_token_ids)?;
        if w.output_token_chain != oczekiwany_lc {
            return Err(SimonError::ReceiptMismatch(
                "output_token_chain niezgodny — to nie ten output".into(),
            ));
        }
        Ok(())
    }

    /// Odcisk treści receiptu. Podpis nie wchodzi do odcisku.
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        content_digest(&unsigned)
    }

    /// Podpisuje receipt kluczem node'a. Ustawia `signer` z klucza.
    pub fn sign(mut self, keypair: &Keypair) -> Result<Self, SimonError> {
        self.signer = keypair.public();
        let digest = self.digest()?;
        self.signature = Some(keypair.sign_digest(&digest));
        Ok(self)
    }

    /// Weryfikuje podpis pod kluczem ZAWIARTYM w receipcie.
    ///
    /// To NIE wystarcza do przyjęcia receiptu — koordynator musi jeszcze
    /// sprawdzić, że `signer` to klucz zarejestrowany (patrz `verify_for_node`).
    pub fn verify_self(&self) -> Result<(), SimonError> {
        let sig = self.signature.as_ref().ok_or(SimonError::BadSignature)?;
        let digest = self.digest()?;
        self.signer.verify_digest(&digest, sig)
    }

    /// Wariant C — pełna weryfikacja wobec zarejestrowanego klucza node'a.
    ///
    /// Trzy bramki. Każda musi przejść. To zamyka F3.
    pub fn verify_for_node(
        &self,
        expected_job_id: &str,
        expected_node_id: &str,
        registered_key: &PublicKey,
    ) -> Result<(), SimonError> {
        // Bramka 1: podpis ważny pod kluczem z receiptu.
        self.verify_self()?;

        // Bramka 2: klucz z receiptu == klucz zarejestrowany.
        if &self.signer != registered_key {
            return Err(SimonError::UnregisteredKey);
        }

        // Bramka 3: receipt dotyczy tego joba i tego node'a.
        if self.job_id != expected_job_id {
            return Err(SimonError::ReceiptMismatch(format!(
                "job_id: receipt={} oczekiwano={}",
                self.job_id, expected_job_id
            )));
        }
        if self.node_id != expected_node_id {
            return Err(SimonError::ReceiptMismatch(format!(
                "node_id: receipt={} oczekiwano={}",
                self.node_id, expected_node_id
            )));
        }
        Ok(())
    }

    /// Czas wykonania w sekundach (z mikrosekund). TopLoc §5: walidacja to
    /// jeden prefill, więc jest rząd wielkości tańsza niż generacja.
    /// **NIE UŻYWAĆ do niczego wiążącego** (audyt 2026-09-18).
    ///
    /// Liczy różnicę dwóch znaczników z ZEGARA ŚCIENNEGO węzła. Poza tym, że
    /// węzeł może je ustawić dowolnie, taki zegar bywa zawodny bez złej woli:
    /// NTP potrafi skokowo cofnąć lub przesunąć czas, więc różnica może wyjść
    /// losowa albo zerowa (mamy `saturating_sub`, więc ujemna zamienia się
    /// w zero — co maskuje problem zamiast go pokazać).
    ///
    /// Czas trwania mierzy się zegarem MONOTONICZNYM po stronie, która mierzy,
    /// z jawnie określonymi granicami — patrz `obserwacja::ClientObservationV1`.
    /// Nigdy nie odejmujemy od siebie znaczników z dwóch różnych maszyn.
    ///
    /// Te pola zostają w receipcie jako orientacyjny znacznik do logu
    /// (wire format zamrożony do M5.1c), ale nie zasilają żadnej decyzji.
    #[deprecated(
        note = "różnica zegarów ściennych węzła — do logu, nie do rozliczeń; \
                czas trwania bierz z ClientObservationV1"
    )]
    pub fn elapsed_secs(&self) -> f64 {
        (self.finished_at_us.saturating_sub(self.started_at_us)) as f64 / 1_000_000.0
    }
}

#[cfg(test)]
mod testy_m1 {
    use super::*;
    use crate::crypto::Keypair;

    fn bazowy() -> Receipt {
        Receipt {
            job_id: "job-test".into(),
            node_id: "node-test".into(),
            model_hash: "mistral-7b".into(),
            runtime: "llama.cpp/0.3".into(),
            precision: Precision::Fp16,
            activation_hash: "toploc:NIE_POLICZONY".into(),
            output_digest: odcisk_wyjscia("job-test", "hello").unwrap(),
            prompt_tokens: 3,
            completion_tokens: 2,
            started_at_us: 1,
            finished_at_us: 2,
            signer: Keypair::from_seed(&[7u8; 32]).public(),
            signature: None,
            wiazania: None,
        }
    }

    /// Regresja kluczowa: dodanie pól M1 (opcjonalnych) NIE MOŻE zmienić
    /// odcisku receiptu legacy — inaczej wszystkie stare podpisy padają.
    #[test]
    fn stary_receipt_z_przykladow_dalej_weryfikuje_podpis() {
        let r: Receipt =
            serde_json::from_str(include_str!("../../../przyklady/receipt.json").trim()).unwrap();
        assert_eq!(r.poziom(), 0);
        assert!(r.verify_self().is_ok(), "podpis legacy musi dalej działać");
        assert!(r.zgodny_z_wyjsciem(include_str!("../../../przyklady/odpowiedz.txt")));
    }

    #[test]
    fn receipt_legacy_bez_pol_m1_serializuje_sie_staro() {
        let s = serde_json::to_string(&bazowy()).unwrap();
        for pole in [
            "schema_version",
            "receipt_level",
            "exec_profile",
            "tokenizer_hash",
            "prompt_digest",
            "output_token_chain",
            "client_nonce",
            "sampling_params_hash",
            "rng_seed",
        ] {
            assert!(!s.contains(pole), "pole {pole} nie może się serializować gdy None: {s}");
        }
    }

    #[test]
    fn m1_przechodzi_i_tamper_lamie() {
        let k = Keypair::from_seed(&[9u8; 32]);
        let prompt = [10u32, 20, 30];
        let out = [40u32, 50, 60];
        let r = bazowy()
            .z_wiazaniem_m1(
                "tokhash".into(),
                "nonce-1".into(),
                &prompt,
                &out,
                "params".into(),
                42,
                "llama.cpp/Q4_K_M/cpu".into(),
            )
            .unwrap()
            .sign(&k)
            .unwrap();
        assert_eq!(r.poziom(), 1);
        assert!(r.verify_self().is_ok());
        assert!(r.zweryfikuj_m1("tokhash", "nonce-1", &prompt, &out).is_ok());
        let mut out2 = out;
        out2[1] = 999;
        assert!(r.zweryfikuj_m1("tokhash", "nonce-1", &prompt, &out2).is_err());
        assert!(r.zweryfikuj_m1("tokhash", "nonce-2", &prompt, &out).is_err());
        assert!(r.zweryfikuj_m1("tokhash-x", "nonce-1", &prompt, &out).is_err());
    }

    #[test]
    fn sampling_commitment_jest_kanoniczny_i_bez_floatow() {
        let c = SamplingCommitment::greedy();
        let d1 = c.odcisk().unwrap();
        let d2 = SamplingCommitment::greedy().odcisk().unwrap();
        assert_eq!(d1, d2, "ten sam commitment = ten sam odcisk");
        let mut c2 = c.clone();
        c2.top_p_x1e6 = 900_000;
        assert_ne!(d1, c2.odcisk().unwrap());
        // Serde nie może wyprodukować floatów (podpisywana treść bez floatów).
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains('.'), "commitment nie może zawierać floatów: {s}");
    }

    #[test]
    fn domeny_sa_rozdzielone() {
        assert_ne!(
            odcisk_promptu("t", "n", &[1, 2, 3]).unwrap(),
            lancuch_tokenow("t", &[1, 2, 3]).unwrap()
        );
        assert_ne!(
            odcisk_parametrow(0, 1000, 1, 0, 1000).unwrap(),
            odcisk_parametrow(1, 1000, 1, 0, 1000).unwrap()
        );
        assert_eq!(odcisk_tokenizera(b"abc"), odcisk_tokenizera(b"abc"));
        assert_ne!(odcisk_tokenizera(b"abc"), odcisk_tokenizera(b"abd"));
    }
}
