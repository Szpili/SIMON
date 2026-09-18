//! M5.2 — lokalny, dopisywalny metrycznik podpisanej pracy.
//!
//! **To NIE jest portfel i NIE jest saldo.** Rejestr zapisuje, co się wydarzyło
//! i na jakim poziomie zostało sprawdzone. Nie emituje nagród, nie prowadzi
//! salda wymienialnych praw do przyszłej pracy i nie ustala przelicznika —
//! bo „wykonane i zweryfikowane" nie znaczy „kupione przez niezależny popyt".
//! Dopóki wash-compute jest nierozwiązany, saldo byłoby zaproszeniem do farmy.
//!
//! Format: JSON Lines, wyłącznie dopisywanie. Plik tekstowy da się obejrzeć
//! bez naszych narzędzi — to jest cecha, nie niedoróbka.
//!
//! **Pola, których dziś nie mamy, są `None`, a nie wypełnione zaślepką.**
//! Zaślepka wyglądałaby jak dane i po tygodniu nikt by nie pamiętał, że nie są.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::crypto::{PublicKey, Signature};

/// Do jakiego poziomu doszła weryfikacja. Jedno `verified: true` skleiłoby
/// podpis, integralność treści i poprawność obliczenia w jedno mylące słowo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusWeryfikacji {
    /// Podpis Ed25519 się zgadza. Nic więcej.
    SignatureValid,
    /// Dodatkowo: odcisk pokrywa odebraną treść.
    OutputBound,
    /// Wysłane do audytu wykonania, brak werdyktu.
    ExecutionAuditPending,
    ExecutionAuditPass,
    ExecutionAuditFail,
}

/// Jedna wykonana jednostka pracy — tak, jak ją widział KLIENT.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RekordPracyV1 {
    pub schema: u32,
    pub receipt_hash: String,
    pub job_id: String,

    pub client_pubkey: PublicKey,
    pub executor_pubkey: PublicKey,

    /// Nazwa modelu ZADEKLAROWANA przez węzeł. To string, nie hash manifestu —
    /// manifestu modelu nie mamy, więc pole poniżej jest `None`.
    pub model_declared: String,
    pub runtime_declared: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokenizer_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_profile_hash: Option<String>,

    /// Prompt nie jest dziś objęty commitmentem — stąd `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_commitment: Option<String>,
    pub output_commitment: String,

    /// Liczniki ZADEKLAROWANE przez węzeł. Nikt ich niezależnie nie przeliczył
    /// (M5.1b). Rozbicie na policzone i z cache'u wymaga danych, których
    /// backendy dziś nie przekazują — stąd `None`, nie zero.
    pub prompt_tokens_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_computed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_cached: Option<u64>,
    pub completion_tokens: u64,

    /// Czasy ZMIERZONE PRZEZ KLIENTA. Deklaracji węzła tu nie ma — celowo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_observed_ttft_ms: Option<u64>,
    pub client_observed_total_ms: u64,

    /// Epoka tożsamości klienta.
    ///
    /// `0` (domyślne przy odczycie starych rekordów) = **LEGACY_EPHEMERAL_IDENTITY**:
    /// rekord podpisał klucz generowany na jedno uruchomienie. Taki rekord dowodzi,
    /// że KONKRETNY efemeryczny klucz podpisał pracę — ale **własności nie da się
    /// odzyskać** (`ownership: UNRECOVERABLE`). Nowa tożsamość nie może
    /// kryptograficznie udowodnić, że kontrolowała tamte klucze, więc
    /// **nie wolno przepisać starych rekordów pod nowy klucz**.
    ///
    /// `1` = trwała tożsamość klienta (M5.2a).
    #[serde(default)]
    pub identity_epoch: u32,
    pub verification_status: StatusWeryfikacji,
    pub receipt_signature: Signature,
}

#[derive(Debug)]
pub enum BladRejestru {
    /// Ten receipt już jest w rejestrze. To jest wykrywanie powtórzeń,
    /// którego brakowało — whitepaper twierdził, że klient sprawdza
    /// jednorazowość receiptu, a nie sprawdzał nic takiego.
    Powtorzenie { receipt_hash: String },
    We(std::io::Error),
    Format(serde_json::Error),
}

impl std::fmt::Display for BladRejestru {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BladRejestru::Powtorzenie { receipt_hash } => {
                write!(f, "receipt {} jest już w rejestrze (powtórzenie)", &receipt_hash[..16.min(receipt_hash.len())])
            }
            BladRejestru::We(e) => write!(f, "wejście/wyjście: {e}"),
            BladRejestru::Format(e) => write!(f, "zły format rekordu: {e}"),
        }
    }
}

/// Dopisywalny rejestr w pliku JSON Lines.
pub struct Rejestr {
    sciezka: PathBuf,
    widziane: HashSet<String>,
}

impl Rejestr {
    /// Otwiera (albo zakłada) rejestr i wczytuje odciski już zapisanych
    /// receiptów, żeby wykryć powtórzenie.
    pub fn otworz(sciezka: impl AsRef<Path>) -> Result<Self, BladRejestru> {
        let sciezka = sciezka.as_ref().to_path_buf();
        let mut widziane = HashSet::new();
        if sciezka.exists() {
            let plik = std::fs::File::open(&sciezka).map_err(BladRejestru::We)?;
            for linia in BufReader::new(plik).lines() {
                let linia = linia.map_err(BladRejestru::We)?;
                if linia.trim().is_empty() {
                    continue;
                }
                // Uszkodzona linia nie może ukryć powtórzenia: przerywamy,
                // zamiast po cichu pominąć.
                let r: RekordPracyV1 = serde_json::from_str(&linia).map_err(BladRejestru::Format)?;
                widziane.insert(r.receipt_hash);
            }
        }
        Ok(Self { sciezka, widziane })
    }

    pub fn dopisz(&mut self, rekord: &RekordPracyV1) -> Result<(), BladRejestru> {
        if self.widziane.contains(&rekord.receipt_hash) {
            return Err(BladRejestru::Powtorzenie {
                receipt_hash: rekord.receipt_hash.clone(),
            });
        }
        let linia = serde_json::to_string(rekord).map_err(BladRejestru::Format)?;
        let mut plik = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.sciezka)
            .map_err(BladRejestru::We)?;
        writeln!(plik, "{linia}").map_err(BladRejestru::We)?;
        self.widziane.insert(rekord.receipt_hash.clone());
        Ok(())
    }

    pub fn wczytaj_wszystko(&self) -> Result<Vec<RekordPracyV1>, BladRejestru> {
        if !self.sciezka.exists() {
            return Ok(Vec::new());
        }
        let plik = std::fs::File::open(&self.sciezka).map_err(BladRejestru::We)?;
        let mut out = Vec::new();
        for linia in BufReader::new(plik).lines() {
            let linia = linia.map_err(BladRejestru::We)?;
            if linia.trim().is_empty() {
                continue;
            }
            out.push(serde_json::from_str(&linia).map_err(BladRejestru::Format)?);
        }
        Ok(out)
    }

    /// SUROWE, rozdzielone liczniki. Celowo bez przelicznika — jednostki
    /// rozliczeniowej nie ustalamy na tym etapie (M5.3).
    pub fn podsumowanie(&self) -> Result<Podsumowanie, BladRejestru> {
        let rekordy = self.wczytaj_wszystko()?;
        let mut p = Podsumowanie::default();
        for r in &rekordy {
            p.zlecen += 1;
            if r.identity_epoch == 0 {
                p.epoka_efemeryczna += 1;
            }
            p.prompt_tokens += r.prompt_tokens_total;
            p.completion_tokens += r.completion_tokens;
            p.czas_klienta_ms += r.client_observed_total_ms;
            match r.verification_status {
                StatusWeryfikacji::OutputBound => p.zwiazanych_z_trescia += 1,
                StatusWeryfikacji::SignatureValid => p.tylko_podpis += 1,
                StatusWeryfikacji::ExecutionAuditPass => p.audyt_wykonania_ok += 1,
                StatusWeryfikacji::ExecutionAuditFail => p.audyt_wykonania_zly += 1,
                StatusWeryfikacji::ExecutionAuditPending => p.audyt_w_toku += 1,
            }
        }
        Ok(p)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Podsumowanie {
    pub zlecen: u64,
    /// Ile rekordów pochodzi z epoki efemerycznej — własność nieodzyskiwalna.
    pub epoka_efemeryczna: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub czas_klienta_ms: u64,
    pub tylko_podpis: u64,
    pub zwiazanych_z_trescia: u64,
    pub audyt_w_toku: u64,
    pub audyt_wykonania_ok: u64,
    pub audyt_wykonania_zly: u64,
}

#[cfg(test)]
mod testy {
    use super::*;
    use crate::crypto::Keypair;

    fn rekord(hash: &str) -> RekordPracyV1 {
        let k = Keypair::generate();
        RekordPracyV1 {
            schema: 1,
            receipt_hash: hash.into(),
            job_id: "job-1".into(),
            client_pubkey: k.public(),
            executor_pubkey: k.public(),
            model_declared: "qwen3.8-27b".into(),
            runtime_declared: "vllm/0.27.1".into(),
            model_manifest_hash: None,
            tokenizer_manifest_hash: None,
            execution_profile_hash: None,
            prompt_commitment: None,
            output_commitment: "abc".into(),
            prompt_tokens_total: 62,
            prompt_tokens_computed: None,
            prompt_tokens_cached: None,
            completion_tokens: 30,
            client_observed_ttft_ms: None,
            client_observed_total_ms: 914,
            identity_epoch: 1,
            verification_status: StatusWeryfikacji::OutputBound,
            receipt_signature: k.sign_digest("d"),
        }
    }

    #[test]
    fn powtorzony_receipt_jest_odrzucany() {
        let kat = tempfile::tempdir().expect("katalog");
        let p = kat.path().join("praca.jsonl");
        let mut r = Rejestr::otworz(&p).expect("otwarcie");
        r.dopisz(&rekord("aaa")).expect("pierwszy przechodzi");
        let blad = r.dopisz(&rekord("aaa"));
        assert!(
            matches!(blad, Err(BladRejestru::Powtorzenie { .. })),
            "to jest wykrywanie powtorzen, ktorego whitepaper obiecywal, a nie bylo"
        );
    }

    #[test]
    fn powtorzenie_wykrywane_takze_po_ponownym_otwarciu() {
        let kat = tempfile::tempdir().expect("katalog");
        let p = kat.path().join("praca.jsonl");
        Rejestr::otworz(&p).unwrap().dopisz(&rekord("bbb")).unwrap();
        // Nowy proces, ten sam plik — stan musi przetrwac restart.
        let mut r2 = Rejestr::otworz(&p).expect("ponowne otwarcie");
        assert!(matches!(r2.dopisz(&rekord("bbb")), Err(BladRejestru::Powtorzenie { .. })));
    }

    #[test]
    fn podsumowanie_liczy_surowe_rozdzielone_liczniki() {
        let kat = tempfile::tempdir().expect("katalog");
        let p = kat.path().join("praca.jsonl");
        let mut r = Rejestr::otworz(&p).unwrap();
        r.dopisz(&rekord("c1")).unwrap();
        r.dopisz(&rekord("c2")).unwrap();
        let s = r.podsumowanie().unwrap();
        assert_eq!(s.zlecen, 2);
        assert_eq!(s.prompt_tokens, 124);
        assert_eq!(s.completion_tokens, 60);
        assert_eq!(s.zwiazanych_z_trescia, 2);
        assert_eq!(s.audyt_wykonania_ok, 0, "audytu wykonania NIE MA — nie wolno go naliczyc");
    }

    #[test]
    fn pola_ktorych_nie_mamy_sa_nieobecne_a_nie_zerowe() {
        let r = rekord("d1");
        let json = serde_json::to_string(&r).unwrap();
        for pole in ["model_manifest_hash", "tokenizer_manifest_hash", "prompt_commitment"] {
            assert!(
                !json.contains(pole),
                "{pole} ma byc NIEOBECNE, nie wypelnione zaslepka"
            );
        }
    }
}
