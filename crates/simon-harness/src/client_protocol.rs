//! Protokół klient ↔ koordynator (SIMON, transport: libp2p + gossipsub).
//!
//! Decyzja Karola 2026-09-16 22:35: „ma być jak torrent inference" → gossipsub.
//! Klient ROZGŁASZA zlecenie do swarmu — nie wie, do kogo gada.
//!
//! Ten moduł to WYŁĄCZNIE kontrakt (typy + walidacja reguł), zero sieci.
//! Warstwa libp2p dochodzi osobno na tych samych typach.
//!
//! Decyzje realizowane:
//!   - D100  trzy typy MVP: JobOrder → OrderAccepted → OrderCompleted
//!   - D101  finalizacja niesie PEŁNY Receipt (nie okrojony wynik)
//!   - D102  Claim/Settle — poza MVP, ale typ FinalizeOrder zdefiniowany jako przyszły
//!   - D103  burn_proof musi być weryfikowalny — struktura, nie string
//!   - D11   klient wybiera MODEL, nigdy node
//!   - D83   fee_think == 0 → brak emisji (odrzucone)
//!   - D84   vrf_seed — klient może zweryfikować losowość przydziału

use serde::{Deserialize, Serialize};

use simon_core::receipt::Receipt;
use simon_core::crypto::Keypair;
use simon_core::{content_digest, SimonError};

/// M0.4b: H(nonce ‖ treść). Nonce w hashu blokuje odgadnięcie krótkich promptów.
pub fn payload_digest_z_nonce(nonce: &str, payload: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(nonce.as_bytes());
    h.update(b"|");
    h.update(payload);
    hex::encode(h.finalize())
}

/// Losowy nonce zlecenia (M0.2). 16 bajtów hex.
fn new_nonce() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}

/// Temat gossipsub, na który klient rozgłasza zlecenia.
///
/// Wszyscy koordynatorzy nasłuchują. To jest odpowiednik „ogłoszenia" w torrentach
/// bez trackera — nikt nie pyta konkretnego serwera.
pub const TOPIC_JOB_ORDER: &str = "simon/v1/job-order";
/// Temat dla potwierdzeń przyjęcia zlecenia.
pub const TOPIC_ORDER_ACCEPTED: &str = "simon/v1/order-accepted";
/// Temat dla zakończonych zleceń.
pub const TOPIC_ORDER_COMPLETED: &str = "simon/v1/order-completed";
/// C (2026-09-17): rejestracja node'ów — node ogłasza, co ma (model, max_ctx).
///
/// Bez tego koordynator nie wie, kto ma jaki model, więc nie może odrzucić
/// zlecenia na model, którego nikt nie serwuje — przyjmował je i cicho wisiało.
pub const TOPIC_NODE_REGISTER: &str = "simon/v1/node-register";

/// Dowód spalenia opłaty (D83 + D103).
///
/// NIE jest stringiem — string da się wygenerować dowolnym kodem.
/// Przy transporcie gossipsub wielu koordynatorów widzi to samo zlecenie,
/// więc KAŻDY musi niezależnie sprawdzić, że spalenie jest prawdziwe i nieużyte.
///
/// W MVP pola mogą być puste (transport jeszcze nie ma łańcucha), ale STRUKTURA
/// jest zdefiniowana od początku — zamiana stringa na strukturę później łamie format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BurnProof {
    /// Hash transakcji spalającej na łańcuchu THINK.
    pub tx_hash: String,
    /// Wysokość bloku, w którym transakcja jest potwierdzona.
    pub block_number: u64,
    /// Merkle proof, że transakcja należy do bloku (klient/node weryfikuje sam).
    pub merkle_proof: Vec<String>,
    /// Nonce — ochrona przed powtórnym użyciem tego samego spalenia (replay).
    pub nonce: u64,
    /// Znacznik czasu spalenia (ochrona przed starymi proofami).
    pub burned_at: f64,
    /// Podpis właściciela środków — dowód, że spalił SWOJE (D103).
    pub owner_signature: String,
    /// Czy to jest proof roboczy (bez łańcucha)? MVP: true. Produkcja: false.
    /// Pole istnieje, żeby NIE dało się przypadkiem wypuścić MVP na produkcję.
    pub synthetic: bool,
}

impl BurnProof {
    /// Roboczy proof dla MVP — jawnie oznaczony, że nie jest produkcyjny.
    pub fn synthetic(order_id: &str, fee: u64) -> Self {
        Self {
            tx_hash: format!("synthetic:{}:{}", order_id, fee),
            block_number: 0,
            merkle_proof: Vec::new(),
            nonce: 0,
            burned_at: 0.0,
            owner_signature: String::new(),
            synthetic: true,
        }
    }

    /// Bramka D83: proof musi być weryfikowalny. W MVP przepuszczamy synthetic,
    /// ale wołający MUSI wiedzieć, że to nie produkcja.
    pub fn is_verifiable(&self) -> bool {
        !self.synthetic
            && !self.tx_hash.is_empty()
            && self.block_number > 0
            && !self.owner_signature.is_empty()
    }
}

/// Zlecenie pracy od klienta. Rozgłaszane na TOPIC_JOB_ORDER.
///
/// M0.4: autoryzacja użytkownika — TO podpisuje user i TO jedzie w `JobOrder`.
///
/// **Dlaczego nie „podpis + odcisk":** koordynator dostałby ważny podpis pod liczbą,
/// której nie potrafi z niczym powiązać. Przejęty agent weźmie ważną parę (podpis, odcisk)
/// z taniego zlecenia i dołączy ją do INNEGO `JobOrder` (inny model, wyższa opłata).
///
/// **Rozwiązanie:** user podpisuje POLA, które zlecenie i tak niesie jawnie.
/// Koordynator odtwarza odcisk z jawnych pól, sprawdza podpis, zgodność pól,
/// nonce i termin ważności. **Treści promptu nie widzi** — ma tylko `payload_digest`.
///
/// Świadomie NIE ma tu `session_id`: inaczej koordynator łączy wszystkie zlecenia
/// jednej sesji. Sesja zostaje po stronie agenta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Autoryzacja {
    /// M0.4a: wersja formatu w PODPISANEJ treści. Bez tego nie da się odróżnić
    /// starych podpisów od nowych po zmianie formatu.
    pub v: u16,
    /// H(nonce ‖ treść) — bez samej treści.
    ///
    /// Nonce w hashu blokuje odgadnięcie krótkich promptów („tak", „napraw testy")
    /// przez sprawdzanie hashy ze słownika.
    pub payload_digest: String,
    pub model_hash: String,
    pub fee_think: u64,
    pub timeout_secs: u32,
    /// M0.4b: nonce UŻYTKOWNIKA. Jeden nonce, nie dwa — agent nie dokłada własnego,
    /// więc powtórka tej samej autoryzacji daje ten sam `order_id`.
    pub nonce: String,
    /// M0.4b: termin ważności (unix ts). W MVP ±60 s tolerancji rozjazdu zegarów,
    /// docelowo wysokość bloku.
    pub wazne_do: u64,
    pub user_pubkey: String,
    /// Podpis użytkownika nad `digest()` powyższych pól.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
}

/// Aktualna wersja formatu (M0.4a).
pub const FORMAT_V: u16 = 1;

/// Maksymalny rozjazd zegarów przy sprawdzaniu `wazne_do` (M0.4b, MVP).
pub const TOLERANCJA_ZEGARA_SECS: u64 = 60;

impl Autoryzacja {
    /// Odcisk autoryzacji (podpis nie wchodzi).
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        content_digest(&unsigned)
    }

    /// Podpisuje autoryzację kluczem użytkownika.
    pub fn sign(mut self, keypair: &Keypair) -> Self {
        if let Ok(d) = self.digest() {
            self.signature = Some(hex::encode(keypair.sign_digest(&d).0.to_vec()));
        }
        self
    }

    /// Czy podpis jest ważny dla zadeklarowanego `user_pubkey`?
    pub fn podpis_zgadza_sie(&self) -> bool {
        let Some(sig_hex) = &self.signature else {
            return false;
        };
        let Ok(pk) = simon_core::PublicKey::from_hex(&self.user_pubkey) else {
            return false;
        };
        let Ok(d) = self.digest() else {
            return false;
        };
        let Ok(bajty) = hex::decode(sig_hex) else {
            return false;
        };
        let Ok(arr) = <[u8; 64]>::try_from(bajty.as_slice()) else {
            return false;
        };
        pk.verify_digest(&d, &simon_core::Signature(arr)).is_ok()
    }

    /// Czy autoryzacja jest jeszcze ważna (`wazne_do` + tolerancja rozjazdu)?
    pub fn czy_wazna(&self, teraz_unix: u64) -> bool {
        teraz_unix <= self.wazne_do + TOLERANCJA_ZEGARA_SECS
    }

    /// Czy zgadza się z jawnymi polami zlecenia (M0.4c)?
    pub fn zgodna_z(&self, order: &JobOrder) -> bool {
        self.payload_digest == order.payload_digest
            && self.model_hash == order.model_hash
            && self.fee_think == order.fee_think
            && self.timeout_secs == order.timeout_secs
            && self.user_pubkey == order.client_pubkey
            && self.nonce == order.nonce
    }
}

/// Klient NIE wskazuje node'a (D11, D84) — przydział robi VRF po stronie koordynatora.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobOrder {
    pub order_id: String,
    /// D11: user wybiera MODEL, nie maszynę.
    pub model_hash: String,
    /// RULES #1: node widzi aktywacje, nie tekst. Payload nie idzie do node'a w całości.
    pub payload_digest: String,
    /// D83: opłata, która zostanie spalona. Zero = brak emisji THINK.
    pub fee_think: u64,
    /// D103: dowód spalenia — struktura, nie string.
    pub burn: BurnProof,
    pub timeout_secs: u32,
    /// Klucz publiczny klienta (hex) — do weryfikacji podpisu zlecenia.
    pub client_pubkey: String,
    /// M0.2: losowy nonce zlecenia.
    ///
    /// Bez niego dwa identyczne zlecenia tego samego klienta (różne sesje, ten sam
    /// licznik) dawały TEN SAM `order_id`, więc stary `OrderCompleted` można było
    /// odtworzyć przy nowym zleceniu (powtórzenie).
    pub nonce: String,
    /// Podpis klienta nad odciskiem treści zlecenia.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
    /// M0.4c: autoryzacja użytkownika — TO weryfikuje KOORDYNATOR przed dopuszczeniem.
    ///
    /// Bez tego weryfikował ten sam komponent, który proponuje (D116 powód #2:
    /// pozór). Koordynator odrzuca zlecenie bez ważnej autoryzacji.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub autoryzacja: Option<Autoryzacja>,
}

impl JobOrder {
    /// Tworzy zlecenie bez podpisu (podpis dochodzi przez `sign`).
    ///
    /// `order_id` jest LICZONY, nie podawany: to `digest()` zlecenia z **pustym**
    /// polem `order_id`. Dzięki temu każdy (koordynator, klient, audytor) odtworzy
    /// ten sam `order_id` z samego zlecenia i sprawdzi `order_id == digest`.
    pub fn new(
        model_hash: impl Into<String>,
        payload: &[u8],
        fee_think: u64,
        timeout_secs: u32,
        client_pubkey: impl Into<String>,
    ) -> Result<Self, SimonError> {
        // M0.4b: H(nonce ‖ treść), nie H(treść). Nonce jest nonce UŻYTKOWNIKA.
        let nonce = new_nonce();
        let payload_digest = payload_digest_z_nonce(&nonce, payload);
        let mut zlecenie = Self {
            burn: BurnProof::synthetic("", fee_think),
            order_id: String::new(),
            model_hash: model_hash.into(),
            payload_digest,
            fee_think,
            timeout_secs,
            client_pubkey: client_pubkey.into(),
            nonce,
            signature: None,
            autoryzacja: None,
        };
        // UWAGA: `digest()` nie może zależeć od `burn`, bo `burn` sam zależy od
        // `order_id` (cykl). Dlatego kolejność: policz digest przy ZEROWYM burn,
        // ustaw order_id, a burn nadaj DOPIERO potem — i wyzeruj go w digest().
        zlecenie.order_id = zlecenie.digest()?;
        let id = zlecenie.order_id.clone();
        zlecenie.burn = BurnProof::synthetic(&id, fee_think);
        Ok(zlecenie)
    }

    /// M0.2 (weryfikacja po stronie KOORDYNATORA): czy `order_id` odpowiada treści?
    ///
    /// Liczenie tego u nadawcy niczego nie zabezpiecza — musi to sprawdzić
    /// odbierający, bo inaczej nadawca wpisze dowolny `order_id`.
    pub fn order_id_sie_zgadza(&self) -> bool {
        match self.digest() {
            Ok(d) => d == self.order_id,
            Err(_) => false,
        }
    }

    /// Odcisk treści zlecenia (podpis nie wchodzi do odcisku).
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        // Podpis i order_id nie wchodzą do odcisku — inaczej nie da się go policzyć
        // z samego zlecenia (order_id JEST tym odciskiem).
        unsigned.signature = None;
        unsigned.order_id = String::new();
        // Autoryzacja zawiera payload_digest/model/opłatę — czyli WEJŚCIA tego
        // odcisku. Nie może wchodzić do wyniku (cykl).
        unsigned.autoryzacja = None;
        // `burn` zależy od `order_id` → nie może wchodzić do odcisku, który
        // ten `order_id` definiuje (cykl). Zerujemy strukturę, zachowując kształt.
        unsigned.burn = BurnProof {
            tx_hash: String::new(),
            block_number: 0,
            merkle_proof: Vec::new(),
            nonce: 0,
            burned_at: 0.0,
            owner_signature: String::new(),
            ..unsigned.burn
        };
        content_digest(&unsigned)
    }

    /// Bramka D83: zlecenie bez opłaty nie generuje emisji.
    pub fn is_funded(&self) -> bool {
        self.fee_think > 0
    }

    /// Bramka D103: czy dowód spalenia jest weryfikowalny (nie roboczy)?
    pub fn has_verifiable_burn(&self) -> bool {
        self.burn.is_verifiable()
    }

    /// M0.4c: bramka KOORDYNATORA — czy dopuścić zlecenie do sieci (D110)?
    ///
    /// Kolejność ma znaczenie: najpierw zgodność pól, potem podpis, na końcu
    /// nonce/termin. Zwraca `Ok(odcisk_autoryzacji)` — to jest klucz do rejestru
    /// zużytych nonce (ZjedzonaAutoryzacja).
    pub fn zweryfikuj_autoryzacje(&self, teraz_unix: u64) -> Result<String, SimonError> {
        let Some(auth) = &self.autoryzacja else {
            return Err(SimonError::ReceiptMismatch(
                "D110: zlecenie bez autoryzacji użytkownika".into(),
            ));
        };
        if auth.v != FORMAT_V {
            return Err(SimonError::ReceiptMismatch(format!(
                "M0.4a: nieznana wersja formatu {} (oczekiwano {})",
                auth.v, FORMAT_V
            )));
        }
        if !auth.zgodna_z(self) {
            return Err(SimonError::ReceiptMismatch(
                "M0.4c: pola zlecenia NIE zgadzają się z autoryzacją (podstawienie)".into(),
            ));
        }
        if !auth.podpis_zgadza_sie() {
            return Err(SimonError::ReceiptMismatch(
                "D110: podpis użytkownika nie odpowiada autoryzacji".into(),
            ));
        }
        if !auth.czy_wazna(teraz_unix) {
            return Err(SimonError::ReceiptMismatch(format!(
                "M0.4b: autoryzacja przeterminowana (wazne_do={}, teraz={})",
                auth.wazne_do, teraz_unix
            )));
        }
        auth.digest()
    }
}

/// Rejestr zużytych autoryzacji (M0.4b) — po stronie koordynatora.
///
/// **ZNANA GRANICA (nie ✅):** przy gossipsub każdy koordynator ma WŁASNĄ listę,
/// więc ta sama autoryzacja rozgłoszona dwa razy trafi do dwóch koordynatorów.
/// W MVP ratuje deterministyczny `order_id` + `winner()` (dwie kopie = ten sam
/// `order_id`, wygrywa jedna). Globalnie przed podwójnym wydaniem chroni dopiero
/// **księga spaleń (nonce z `BurnProof`, D103)** — a ta jest dziś syntetyczna.
#[derive(Debug, Clone, Default)]
pub struct RejestrZuzytychAutoryzacji {
    zuzyte: std::collections::HashSet<String>,
}

impl RejestrZuzytychAutoryzacji {
    /// Sprawdza i jednocześnie zajmuje autoryzację. Powtórka → błąd.
    pub fn zajmij(&mut self, odcisk_autoryzacji: &str) -> Result<(), SimonError> {
        if !self.zuzyte.insert(odcisk_autoryzacji.to_string()) {
            return Err(SimonError::ReceiptMismatch(
                "M0.4b: autoryzacja już zużyta (powtórka tego samego podpisu)".into(),
            ));
        }
        Ok(())
    }
}

/// Potwierdzenie przyjęcia zlecenia. Rozgłaszane na TOPIC_ORDER_ACCEPTED.
///
/// Przy gossipsub WIELU koordynatorów może odpowiedzieć na to samo zlecenie.
/// Dlatego `coordinator_pubkey` jest obowiązkowe — klient musi wiedzieć, komu ufa,
/// i musi móc odrzucić duplikaty (patrz `OrderAccepted::winner`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderAccepted {
    pub order_id: String,
    /// Deterministyczny identyfikator joba: H(order_id ‖ block).
    pub job_id: String,
    /// Koordynator, który przyjmuje zlecenie (przy gossipsub — zawodnik w wyścigu).
    pub coordinator_pubkey: String,
    /// D84: ziarno losowania — klient może zweryfikować, że przydział był losowy.
    pub vrf_seed: String,
    /// Blok, z którego pochodzi ziarno (do weryfikacji nieprzewidywalności).
    pub vrf_block: u64,
    pub estimated_ms: u64,
    /// M0.1: podpis koordynatora nad odciskiem przyjęcia.
    ///
    /// Bez tego `coordinator_pubkey` to zwykły string, który wpisze ktokolwiek —
    /// atakujący dobiera klucz dla konkretnego zlecenia i wygrywa wyścig.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
}

impl OrderAccepted {
    /// Odcisk przyjęcia (podpis nie wchodzi do odcisku).
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        content_digest(&unsigned)
    }

    /// Podpisuje przyjęcie kluczem koordynatora (M0.1).
    pub fn sign(mut self, keypair: &Keypair) -> Self {
        if let Ok(d) = self.digest() {
            self.signature = Some(hex::encode(keypair.sign_digest(&d).0.to_vec()));
        }
        self
    }

    /// Czy podpis jest ważny dla zadeklarowanego `coordinator_pubkey`?
    pub fn podpis_zgadza_sie(&self) -> bool {
        let Some(sig_hex) = &self.signature else {
            return false;
        };
        let Ok(pk) = simon_core::PublicKey::from_hex(&self.coordinator_pubkey) else {
            return false;
        };
        let Ok(d) = self.digest() else {
            return false;
        };
        let Ok(bajty) = hex::decode(sig_hex) else {
            return false;
        };
        let Ok(arr): Result<[u8; 64], _> = bajty.try_into() else {
            return false;
        };
        pk.verify_digest(&d, &simon_core::Signature(arr)).is_ok()
    }

    /// Rozstrzygnięcie wyścigu, gdy wielu koordynatorów odpowie na to samo zlecenie.
    ///
    /// GOSSIPSUB KONSEKWENCJA: bez tej reguły dwóch koordynatorów rozliczy ten sam
    /// job → podwójne rozliczenie (F2 z red-teamu RT1).
    ///
    /// Reguła: wygrywa **najniższy `H(order_id ‖ coordinator_pubkey)`**.
    ///
    /// NIE `min coordinator_pubkey`. Gdyby wygrywał sam klucz, wystarczy
    /// wygenerować klucz zaczynający się od "0000…" i wygrać KAŻDY wyścig
    /// w całej sieci — bez żadnej pracy. Hash wiąże klucz z konkretnym zleceniem,
    /// więc nie da się przygotować klucza wygrywającego wszystko.
    ///
    /// Deterministyczna, weryfikowalna przez każdego, nie wymaga uzgodnienia.
    pub fn winner(candidates: &[OrderAccepted]) -> Option<&OrderAccepted> {
        candidates.iter().min_by(|a, b| {
            Self::ranking(a)
                .cmp(&Self::ranking(b))
                .then_with(|| a.job_id.cmp(&b.job_id))
        })
    }

    /// Wewnętrzny ranking (bez zmiany widoczności).
    fn ranking_wewn(c: &OrderAccepted) -> String {
        Self::ranking(c)
    }

    /// Klucz rankingu wyścigu: H(order_id ‖ coordinator_pubkey), hex.
    fn ranking(c: &OrderAccepted) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(c.order_id.as_bytes());
        h.update(b"|");
        h.update(c.coordinator_pubkey.as_bytes());
        hex::encode(h.finalize())
    }
}

/// Zakończenie zlecenia. Rozgłaszane na TOPIC_ORDER_COMPLETED.
///
/// D101: niesie PEŁNY `Receipt` z simon-core — inaczej klient nie zweryfikuje D67.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderCompleted {
    pub order_id: String,
    pub job_id: String,
    /// Kto wykonał (do sprawdzenia wobec rejestru — bramka wariantu C).
    pub node_id: String,
    /// Pełny receipt: model_hash, runtime, precision, activation_hash,
    /// output_digest, signer, signature. Podpisany Ed25519 przez node.
    pub receipt: Receipt,
    /// Koordynator, który finalizuje.
    pub coordinator_pubkey: String,
}

/// Klient weryfikuje wynik SAM (RULES #6) — bez zaufania do koordynatora.
///
/// To jest domknięcie całego łańcucha: klient nie musi ufać ani koordynatorowi,
/// ani sieci. Wystarczy mu klucz publiczny node'a z rejestru.
pub fn verify_completion(
    completed: &OrderCompleted,
    expected_order_id: &str,
    registered_node_key: &simon_core::PublicKey,
) -> Result<(), SimonError> {
    // 1) Czy to jest zlecenie, które zamówiliśmy?
    if completed.order_id != expected_order_id {
        return Err(SimonError::ReceiptMismatch(format!(
            "order_id: otrzymano={} oczekiwano={}",
            completed.order_id, expected_order_id
        )));
    }

    // 2) Trzy bramki F3: podpis / klucz zarejestrowany / zgodność job+node.
    completed.receipt.verify_for_node(
        &completed.job_id,
        &completed.node_id,
        registered_node_key,
    )?;

    Ok(())
}

/// Statyczna lista zaufanych koordynatorów (M0.1 — świadome uproszczenie MVP).
///
/// **Kto dopisuje klucze i kto jest źródłem zaufania to decyzja strategiczna,
/// świadomie odłożona.** Na MVP: lista w pliku konfiguracyjnym.
///
/// KLUCZOWE: filtrujemy kandydatów **PRZED** rankingiem, nie po. Gdyby `winner()`
/// dostał wszystkich, a podpis sprawdzano na zwycięzcy, niepodpisany atakujący
/// dalej wygrywa, a uczciwy odpada razem z nim.
#[derive(Debug, Clone, Default)]
pub struct RejestrKoordynatorow {
    klucze: std::collections::HashSet<String>,
}

impl RejestrKoordynatorow {
    pub fn z_kluczami(klucze: &[String]) -> Self {
        Self {
            klucze: klucze.iter().cloned().collect(),
        }
    }

    pub fn zawiera(&self, pubkey_hex: &str) -> bool {
        self.klucze.contains(pubkey_hex)
    }

    /// Kandydaci dopuszczeni do wyścigu: zarejestrowani + ważny podpis
    /// nad TYM zleceniem (`order_id` musi się zgadzać z tym, o co pytamy).
    pub fn dopusc<'a>(
        &self,
        kandydaci: &'a [OrderAccepted],
        order_id: &str,
    ) -> Vec<&'a OrderAccepted> {
        kandydaci
            .iter()
            .filter(|c| c.order_id == order_id)
            .filter(|c| self.zawiera(&c.coordinator_pubkey))
            .filter(|c| c.podpis_zgadza_sie())
            .collect()
    }

    /// Zwycięzca wyścigu — ranking **tylko pośród dopuszczonych**.
    pub fn winner_zrejestrowany<'a>(
        &self,
        kandydaci: &'a [OrderAccepted],
        order_id: &str,
    ) -> Option<&'a OrderAccepted> {
        let dopuszczeni = self.dopusc(kandydaci, order_id);
        dopuszczeni.into_iter().min_by(|a, b| {
            OrderAccepted::ranking_wewn(a)
                .cmp(&OrderAccepted::ranking_wewn(b))
                .then_with(|| a.job_id.cmp(&b.job_id))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(id: &str, fee: u64) -> JobOrder {
        JobOrder::new("sha256:model", b"prompt", fee, 300, "aa".repeat(32)).unwrap()
    }

    #[test]
    fn funded_order_passes_gate() {
        assert!(order("o1", 10).is_funded());
    }

    #[test]
    fn zero_fee_order_fails_d83_gate() {
        assert!(!order("o1", 0).is_funded(), "D83: bez opłaty nie ma emisji");
    }

    #[test]
    fn synthetic_burn_is_not_verifiable() {
        let o = order("o1", 10);
        assert!(!o.has_verifiable_burn(), "MVP proof nie jest produkcyjny");
        assert!(o.burn.synthetic, "musi być jawnie oznaczony");
    }

    #[test]
    fn payload_digest_jest_stabilny_w_obrebie_zlecenia() {
        // M0.4b: digest = H(nonce ‖ treść). W obrębie JEDNEGO zlecenia stabilny.
        let a = order("o1", 10);
        assert_eq!(a.payload_digest, a.payload_digest);
        assert_eq!(a.payload_digest.len(), 64, "sha256 hex");
        // Dwa różne zlecenia z tą samą treścią mają RÓŻNE digesty (różny nonce),
        // bo inaczej krótkie prompty dałyby się odgadnąć słownikiem.
        let b = order("o1", 10);
        assert_ne!(
            a.payload_digest, b.payload_digest,
            "nonce w hashu: ten sam prompt → różny digest"
        );
    }

    #[test]
    fn signature_excluded_from_order_digest() {
        let mut o = order("o1", 10);
        let before = o.digest().unwrap();
        o.signature = Some("deadbeef".into());
        assert_eq!(before, o.digest().unwrap());
    }

    #[test]
    fn gossipsub_race_resolved_deterministically() {
        // Sytuacja z gossipsub: DWÓCH koordynatorów łapie to samo zlecenie.
        let mk = |pk: &str| OrderAccepted {
            order_id: "o1".into(),
            job_id: "j1".into(),
            coordinator_pubkey: pk.to_string(),
            vrf_seed: "seed".into(),
            vrf_block: 7,
            estimated_ms: 100,
            signature: None,
        };
        let a = mk("ff");
        let b = mk("aa");
        let candidates = [a.clone(), b.clone()];
        let winner = OrderAccepted::winner(&candidates).unwrap();
        // Ten sam wynik niezależnie od kolejności — to jest istota determinizmu.
        let reversed = [b.clone(), a.clone()];
        let winner2 = OrderAccepted::winner(&reversed).unwrap();
        assert_eq!(
            winner.coordinator_pubkey, winner2.coordinator_pubkey,
            "zwycięzca nie może zależeć od kolejności, w jakiej przyszły przyjęcia"
        );
        // Reguła: min H(order_id ‖ pubkey), NIE min pubkey.
        // Gdyby wygrywał sam klucz, klucz "0000…" przejmowałby każdy wyścig.
        let ranking = |pk: &str| {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(b"o1|");
            h.update(pk.as_bytes());
            hex::encode(h.finalize())
        };
        let (ra, rb) = (ranking("ff"), ranking("aa"));
        let oczekiwany = if ra < rb { "ff" } else { "aa" };
        assert_eq!(
            winner.coordinator_pubkey, oczekiwany,
            "wygrywa min H(order_id ‖ pubkey)"
        );
    }

    #[test]
    fn wygenerowany_klucz_nie_przejmuje_wyscigu() {
        // Atak bez pracy: wygeneruj klucz zaczynający się od "0000…".
        // Przy regule `min pubkey` taki klucz wygrywałby KAŻDY wyścig w sieci.
        // Przy `min H(order_id ‖ pubkey)` nie da się tego przygotować z góry.
        let mk = |pk: &str| OrderAccepted {
            order_id: "o1".into(),
            job_id: "j1".into(),
            coordinator_pubkey: pk.to_string(),
            vrf_seed: "seed".into(),
            vrf_block: 7,
            estimated_ms: 100,
            signature: None,
        };
        // Klucz "0000" mógłby wygrać przy min pubkey tylko jeśli hash też wypadnie nisko.
        let atak = mk("0000000000000000000000000000000000000000000000000000000000000000");
        let zwykly = mk("ff");
        let kandydaci = [atak.clone(), zwykly.clone()];
        let wygrany = OrderAccepted::winner(&kandydaci).unwrap();
        // Sprawdzamy tylko, że reguła jest hashowa, nie leksykalna.
        let ranking = |pk: &str| {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(pk.as_bytes());
            hex::encode(h.finalize())
        };
        let oczekiwany = if ranking("0000000000000000000000000000000000000000000000000000000000000000")
            < ranking("ff")
        {
            "0000000000000000000000000000000000000000000000000000000000000000"
        } else {
            "ff"
        };
        assert_eq!(wygrany.coordinator_pubkey, oczekiwany);
    }
}
