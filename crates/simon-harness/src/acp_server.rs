//! Serwer ACP dla SIMON — warstwa interfejsu wobec zewnętrznych harnessów.
//!
//! Decyzje realizowane:
//!   - D105  SIMON jest AGENTEM ACP (nie klientem). Harness widzi jednego agenta;
//!           swarm jest niewidoczny.
//!   - D106  ACP nie ma weryfikacji → receipt jedzie w `Meta`, ale WERYFIKACJA
//!           zostaje w rdzeniu. Klient nie ufa, tylko sprawdza podpis Ed25519.
//!   - D107  Swarm niewidoczny: VRF, wyścig gossipsub, `winner()` nie wychodzą
//!           na zewnątrz. Klient widzi tylko wynik i (opcjonalnie) receipt.
//!   - D108  Klient może zażądać receiptu — przez capability, nie obowiązkowo.
//!   - D110  Agent tylko PROPONUJE zlecenie. Nie autoryzuje. Podpis użytkownika
//!           jest sprawdzany PRZED dopuszczeniem do swarmu.
//!   - D116  `acp_server` to ZWYKŁY KLIENT protokołu SIMON. Wysyła `JobOrder`
//!           na gossipsub i czeka na `OrderAccepted` / `OrderCompleted`.
//!           NIE wie, czy koordynator jest w tym samym procesie, czy na Franku.
//!   - Strumień: `AgentThoughtChunk` (postęp) na żywo, `AgentMessageChunk` (wynik)
//!           DOPIERO po weryfikacji receiptu (konsylium 2026-09-16).
//!   - Sesje: jedna trwała sesja ACP (dla harnessu), każde zlecenie = osobny
//!           `job_id` i osobny receipt (mapowanie SessionId : order_id = 1 : N).

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use simon_core::SimonError;

use crate::client_protocol::{
    OrderAccepted, OrderCompleted, JobOrder, TOPIC_JOB_ORDER,
    TOPIC_ORDER_ACCEPTED, TOPIC_ORDER_COMPLETED,
};
use crate::transport::{SimonEvent, SwarmHandle};

/// Identyfikator sesji ACP (konwersacja klient ↔ agent).
///
/// Jedna sesja trwała; każde zlecenie w sesji ma własny `job_id` i receipt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

/// Faza zlecenia — to, co klient widzi jako postęp.
///
/// To jest mapowanie wewnętrznego stanu SIMON na `AgentThoughtChunk`.
/// Klient widzi POSTĘP, ale nie widzi treści wyniku przed weryfikacją (D106).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FazaZlecenia {
    /// Zlecenie rozgłoszone do swarmu — czekamy na koordynatora.
    Rozgloszone,
    /// Koordynator przyjął (wygraliśmy wyścig przez `winner()`).
    Przyjete { job_id: String },
    /// Wynik dotarł — trwa weryfikacja receiptu.
    Weryfikacja,
    /// Wynik zweryfikowany — można pokazać klientowi.
    Gotowe,
    /// Błąd (z powodem).
    Blad(String),
}

impl FazaZlecenia {
    /// Tekst dla `AgentThoughtChunk` — to widzi klient na żywo.
    pub fn jako_postep(&self) -> String {
        match self {
            Self::Rozgloszone => "Zlecenie rozgłoszone do swarmu…".into(),
            Self::Przyjete { job_id } => format!("Koordynator przyjął zlecenie ({job_id})."),
            Self::Weryfikacja => "Wynik dotarł — weryfikuję receipt…".into(),
            Self::Gotowe => "Wynik zweryfikowany.".into(),
            Self::Blad(e) => format!("Błąd: {e}"),
        }
    }
}

/// Żądanie użytkownika (odpowiednik ACP `PromptRequest`).
///
/// D110: to jest **PROPOZYCJA** zlecenia. Nie zostanie rozgłoszona, dopóki
/// nie przejdzie bramki autoryzacji (podpis użytkownika).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZlecenieUzytkownika {
    pub session_id: SessionId,
    /// Treść zapytania (u klienta, nie w swarmie — RULES #1).
    pub tresc: String,
    /// Model wybrany przez użytkownika (D11: user wybiera MODEL, nie maszynę).
    pub model_hash: String,
    /// Ile użytkownik płaci. Opłata jest SPALONA (D83).
    pub fee_think: u64,
    pub timeout_secs: u32,
    /// Klucz publiczny użytkownika (hex) — D110, do weryfikacji autoryzacji.
    pub user_pubkey: String,
}

impl ZlecenieUzytkownika {
    /// Odcisk treści zlecenia — to user podpisuje (M0.3, D110).
    ///
    /// Bez tego bramka nie miała CZEGO weryfikować: dowolny niepusty string
    /// przechodził jako „podpis".
    pub fn digest(&self) -> Result<String, SimonError> {
        simon_core::content_digest(self)
    }
}

/// Wynik zlecenia gotowy do pokazania klientowi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WynikZlecenia {
    pub order_id: String,
    pub job_id: String,
    /// Treść wyniku. Wysyłana DOPIERO po weryfikacji receiptu (D106 + konsylium).
    pub tresc: String,
    /// Receipt — klient weryfikuje sam (RULES #6, D108).
    pub receipt: simon_core::receipt::Receipt,
}

/// Bramka autoryzacji (D110).
///
/// **Agent tylko proponuje. Nie autoryzuje.**
///
/// To jest świadomie osobna funkcja, a nie metoda na `acp_server` — bo gdyby
/// proponujący sam zatwierdzał, D110 byłoby pozorne (patrz D116, powód #2).
///
/// W MVP: sprawdzenie, że podpis użytkownika istnieje i nie jest pusty.
/// M0.3: weryfikacja Ed25519 nad odciskiem zlecenia (zrealizowane).
pub trait BramkaAutoryzacji: Send + Sync {
    /// Zwraca `Ok(())`, jeśli zlecenie jest autoryzowane przez użytkownika.
    fn autoryzuj(&self, zlecenie: &ZlecenieUzytkownika, podpis: &str) -> Result<(), SimonError>;
}

/// Bramka produkcyjna: wymaga niepustego podpisu (D110).
///
/// Nie weryfikuje jeszcze kryptograficznie — to dochodzi, gdy klient zacznie
/// podpisywać (wymaga uzgodnienia formatu z harnessem). Ale **brak podpisu
/// już teraz blokuje rozgłoszenie**, więc ścieżka D110 jest przechodnia.
#[derive(Default)]
pub struct BramkaPodpisu;

impl BramkaAutoryzacji for BramkaPodpisu {
    fn autoryzuj(&self, zlecenie: &ZlecenieUzytkownika, podpis: &str) -> Result<(), SimonError> {
        // D110: agent proponuje, user autoryzuje PODPISEM.
        //
        // Wcześniej ta bramka sprawdzała tylko, czy string nie jest pusty —
        // czyli była TEATREM: dowolny niepusty tekst przechodził. Teraz
        // weryfikujemy kryptograficznie (Ed25519 z simon-core::crypto).
        if zlecenie.user_pubkey.trim().is_empty() {
            return Err(SimonError::ReceiptMismatch(
                "D110: brak klucza publicznego użytkownika".into(),
            ));
        }
        if podpis.trim().is_empty() {
            return Err(SimonError::ReceiptMismatch(
                "D110: brak podpisu użytkownika → zlecenie NIE może być rozgłoszone".into(),
            ));
        }

        // Podpis musi być nad ODCISKIEM ZLECENIA, nie nad czymkolwiek.
        let odcisk = zlecenie.digest()?;

        let pk = simon_core::PublicKey::from_hex(&zlecenie.user_pubkey).map_err(|_| {
            SimonError::ReceiptMismatch("D110: klucz użytkownika nie jest poprawnym hexem".into())
        })?;

        let bajty = hex::decode(podpis.trim()).map_err(|_| {
            SimonError::ReceiptMismatch("D110: podpis nie jest poprawnym hexem".into())
        })?;
        let dlugosc = bajty.len();
        let arr: [u8; 64] = bajty.try_into().map_err(|_| {
            SimonError::ReceiptMismatch(format!(
                "D110: podpis Ed25519 ma 64 bajty, otrzymano {}",
                dlugosc
            ))
        })?;

        pk.verify_digest(&odcisk, &simon_core::Signature(arr))
            .map_err(|_| {
                SimonError::ReceiptMismatch(
                    "D110: podpis użytkownika NIE odpowiada treści zlecenia".into(),
                )
            })
    }
}

/// Serwer ACP — klient protokołu SIMON wobec swarmu (D105 + D116).
///
/// **Świadomie nie ma tu referencji do koordynatora.** Publikuje `JobOrder`
/// na gossipsub i czeka na zdarzenia z transportu. Nie wie i nie pyta,
/// gdzie siedzi koordynator.
pub struct AcpServer {
    swarm: SwarmHandle,
    bramka: Box<dyn BramkaAutoryzacji>,
    /// Aktywne zlecenia: order_id → (złożone zlecenie, zwycięskie przyjęcie, faza).
    ///
    /// Trzymamy CAŁE `JobOrder`, bo weryfikacja wyniku musi wiedzieć, CZEGO
    /// dotyczy (model, digest treści) — nie wystarczy samo `order_id`.
    aktywne: HashMap<String, AktywneZlecenie>,
    /// Autoryzowane zlecenia czekające na rozgłoszenie.
    do_rozgloszenia: Vec<JobOrder>,
}

/// Stan jednego aktywnego zlecenia (patrz P1 z odpowiedzi Opusa 2026-09-16).
#[derive(Debug, Clone)]
pub struct AktywneZlecenie {
    /// Zlecenie, które MY złożyliśmy — źródło prawdy dla weryfikacji.
    pub order: JobOrder,
    /// Zwycięskie przyjęcie (rozstrzygnięcie wyścigu przez `winner()`).
    pub accepted: Option<OrderAccepted>,
    /// Aktualna faza (postęp dla klienta).
    pub faza: FazaZlecenia,
}

impl AcpServer {
    pub fn new(swarm: SwarmHandle, bramka: Box<dyn BramkaAutoryzacji>) -> Self {
        Self {
            swarm,
            bramka,
            aktywne: HashMap::new(),
            do_rozgloszenia: Vec::new(),
        }
    }

    /// Przyjmuje zlecenie od użytkownika (odpowiednik `PromptRequest`).
    ///
    /// D110: **najpierw autoryzacja, potem cokolwiek innego.** Zlecenie bez
    /// podpisu nie powstaje nawet jako `JobOrder`.
    ///
    /// D83: opłata 0 → brak emisji THINK → zlecenie odrzucone.
    pub fn przyjmij(
        &mut self,
        zlecenie: ZlecenieUzytkownika,
        podpis: &str,
    ) -> Result<String, SimonError> {
        // Bramka D110 — PRZED czymkolwiek.
        self.bramka.autoryzuj(&zlecenie, podpis)?;

        if zlecenie.fee_think == 0 {
            return Err(SimonError::ReceiptMismatch(
                "D83: opłata 0 → brak emisji THINK".into(),
            ));
        }

        // M0.2: `order_id` jest LICZONY przez `JobOrder::new` (digest zlecenia
        // z pustym order_id + losowy nonce). Agent go nie nadaje — inaczej
        // koordynator nie miałby czym sprawdzić, że order_id odpowiada treści.
        let order = JobOrder::new(
            &zlecenie.model_hash,
            zlecenie.tresc.as_bytes(),
            zlecenie.fee_think,
            zlecenie.timeout_secs,
            &zlecenie.user_pubkey,
        )?;
        let order_id = order.order_id.clone();

        self.aktywne.insert(
            order_id.clone(),
            AktywneZlecenie {
                order: order.clone(),
                accepted: None,
                faza: FazaZlecenia::Rozgloszone,
            },
        );
        self.do_rozgloszenia.push(order);
        Ok(order_id)
    }

    /// Rozgłasza wszystkie autoryzowane, jeszcze nierozgłoszone zlecenia.
    ///
    /// D116: publikacja idzie na gossipsub — nie do konkretnego koordynatora.
    pub fn rozglos(&mut self) -> Result<usize, SimonError> {
        let mut n = 0;
        for order in self.do_rozgloszenia.drain(..) {
            let bajty = serde_json::to_vec(&order).map_err(|e| {
                SimonError::ReceiptMismatch(format!("serializacja zlecenia: {e}"))
            })?;
            if let Err(e) = self.swarm.publish(TOPIC_JOB_ORDER, bajty) {
                return Err(SimonError::ReceiptMismatch(format!(
                    "publikacja zlecenia nie powiodła się: {e}"
                )));
            }
            n += 1;
        }
        Ok(n)
    }

    /// Przetwarza event z transportu i zwraca fazę do pokazania klientowi.
    ///
    /// To jest mapowanie sieci na `AgentThoughtChunk` (konsylium: postęp na żywo).
    pub fn obsluz_event(&mut self, event: &SimonEvent) -> Option<(String, FazaZlecenia)> {
        match event {
            SimonEvent::Message { topic, data, .. } if topic == TOPIC_ORDER_ACCEPTED => {
                let acc: OrderAccepted = serde_json::from_slice(data).ok()?;
                let order_id = acc.order_id.clone();
                if !self.aktywne.contains_key(&order_id) {
                    return None; // nie nasze zlecenie
                }
                // P5: rozstrzygamy wyścig PO STRONIE KLIENTA. Transport nie wie,
                // co znaczy „przyjęte". Bez tego wygrywa OSTATNIE przyjęcie,
                // a spóźnione `Accepted` potrafi cofnąć fazę `Weryfikacja`.
                let zwyciezca = {
                    let stan = self.aktywne.get(&order_id)?;
                    let mut kandydaci: Vec<OrderAccepted> = Vec::new();
                    if let Some(prev) = &stan.accepted {
                        kandydaci.push(prev.clone());
                    }
                    kandydaci.push(acc.clone());
                    OrderAccepted::winner(&kandydaci).cloned()
                };
                let zwyciezca = zwyciezca?;
                let faza = FazaZlecenia::Przyjete {
                    job_id: zwyciezca.job_id.clone(),
                };
                if let Some(stan) = self.aktywne.get_mut(&order_id) {
                    // Nie cofaj fazy, jeśli wynik już dotarł.
                    if matches!(stan.faza, FazaZlecenia::Weryfikacja | FazaZlecenia::Gotowe) {
                        return None;
                    }
                    stan.accepted = Some(zwyciezca);
                    stan.faza = faza.clone();
                }
                Some((order_id, faza))
            }
            SimonEvent::Message { topic, data, .. } if topic == TOPIC_ORDER_COMPLETED => {
                let done: OrderCompleted = serde_json::from_slice(data).ok()?;
                let order_id = done.order_id.clone();
                if !self.aktywne.contains_key(&order_id) {
                    return None;
                }
                // Faza Weryfikacja — wynik dotarł, ale NIE pokazujemy treści (D106).
                let faza = FazaZlecenia::Weryfikacja;
                if let Some(stan) = self.aktywne.get_mut(&order_id) {
                    stan.faza = faza.clone();
                }
                Some((order_id, faza))
            }
            _ => None,
        }
    }

    /// Kończy zlecenie: bierze `OrderCompleted`, weryfikuje receipt i zwraca wynik.
    ///
    /// **To jest miejsce, gdzie D106 obowiązuje.** Treść jest zwracana DOPIERO
    /// po tym, jak `verify_completion` przejdzie. Przedtem klient dostał tylko
    /// `FazaZlecenia::Weryfikacja`.
    ///
    /// D108: klient może zażądać receiptu — tu jest ZAWSZE, bo weryfikacja
    /// i tak go potrzebuje. Różnica wobec D108 polega na tym, że klient
    /// może go *zignorować*; serwer go nie ukrywa.
    pub fn zakoncz(
        &mut self,
        completed: &OrderCompleted,
        klucz_node: &simon_core::PublicKey,
        tresc_wyniku: &str,
    ) -> Result<WynikZlecenia, SimonError> {
        // ── Weryfikacja SAMODZIELNA (RULES #6) — bez zaufania do koordynatora ──
        //
        // Sprawdzamy wobec zlecenia, które MY złożyliśmy. Samo `order_id` nie
        // wystarczy: atakujący może podstawić wynik z POPRAWNYM order_id, ale
        // innym job_id, innym modelem albo niepowiązaną treścią (P1 z Opusa).

        // 1) Czy to nasze zlecenie w ogóle?
        let stan = self.aktywne.get(&completed.order_id).ok_or_else(|| {
            SimonError::ReceiptMismatch(format!(
                "D106: wynik dla nieznanego zlecenia {}",
                completed.order_id
            ))
        })?;
        let order = stan.order.clone();
        let przyjete = stan.accepted.clone();

        // 2) Czy zwycięskie przyjęcie zgadza się z tym, co przyszło?
        //    (inaczej: wynik z innego koordynatora niż ten, który wygrał wyścig)
        if let Some(acc) = &przyjete {
            if acc.job_id != completed.job_id {
                return Err(SimonError::ReceiptMismatch(format!(
                    "job_id: wynik={} wygrane_przyjecie={}",
                    completed.job_id, acc.job_id
                )));
            }
            if acc.coordinator_pubkey != completed.coordinator_pubkey {
                return Err(SimonError::ReceiptMismatch(format!(
                    "koordynator: wynik={} wyścig={}",
                    completed.coordinator_pubkey, acc.coordinator_pubkey
                )));
            }
        }

        // 3) Czy job_id odpowiada NASZEMU order_id? (podstawienie zlecenia)
        let oczekiwany_job = crate::transport::deterministic_job_id(&order.order_id);
        if completed.job_id != oczekiwany_job {
            return Err(SimonError::ReceiptMismatch(format!(
                "job_id nie odpowiada order_id: oczekiwano={} otrzymano={}",
                oczekiwany_job, completed.job_id
            )));
        }

        // 4) Czy receipt dotyczy MODELU, o który prosiliśmy? (D11, D67)
        if completed.receipt.model_hash != order.model_hash {
            return Err(SimonError::ReceiptMismatch(format!(
                "model: receipt={} zlecenie={}",
                completed.receipt.model_hash, order.model_hash
            )));
        }

        // 5) Bramki F3: podpis / klucz zarejestrowany / zgodność job+node.
        crate::client_protocol::verify_completion(completed, &order.order_id, klucz_node)?;

        // 6) NAJWAŻNIEJSZE: czy pokazywana treść to TA, którą podpisano?
        //    Bez tego ważny receipt z dowolnym tekstem przechodzi weryfikację.
        {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(tresc_wyniku.as_bytes());
            let odcisk = hex::encode(h.finalize());
            if odcisk != completed.receipt.output_digest {
                return Err(SimonError::ReceiptMismatch(format!(
                    "output_digest: treść={} receipt={}",
                    odcisk, completed.receipt.output_digest
                )));
            }
        }

        if let Some(stan) = self.aktywne.get_mut(&completed.order_id) {
            stan.faza = FazaZlecenia::Gotowe;
        }

        Ok(WynikZlecenia {
            order_id: completed.order_id.clone(),
            job_id: completed.job_id.clone(),
            tresc: tresc_wyniku.to_string(),
            receipt: completed.receipt.clone(),
        })
    }

    /// Bieżąca faza zlecenia (do wysłania jako postęp).
    pub fn faza(&self, order_id: &str) -> Option<&FazaZlecenia> {
        self.aktywne.get(order_id).map(|a| &a.faza)
    }

    /// Czeka na zakończenie zlecenia i zwraca event `OrderCompleted`.
    ///
    /// Timeout jest po stronie klienta (D83: `timeout_secs` w zleceniu).
    pub async fn czekaj_na_zakonczenie(
        rx: &mut mpsc::UnboundedReceiver<SimonEvent>,
        order_id: &str,
        limit: Duration,
    ) -> Option<OrderCompleted> {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let pozostalo = deadline.saturating_duration_since(tokio::time::Instant::now());
            if pozostalo.is_zero() {
                return None;
            }
            match tokio::time::timeout(pozostalo, rx.recv()).await {
                Ok(Some(SimonEvent::Message { topic, data, .. }))
                    if topic == TOPIC_ORDER_COMPLETED =>
                {
                    if let Ok(done) = serde_json::from_slice::<OrderCompleted>(&data) {
                        if done.order_id == order_id {
                            return Some(done);
                        }
                    }
                }
                Ok(Some(_)) => continue,
                Ok(None) | Err(_) => return None,
            }
        }
    }
}
