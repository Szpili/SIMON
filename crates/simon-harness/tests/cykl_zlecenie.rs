//! Testy integracyjne: pełny cykl zlecenie → wykonanie → weryfikacja przez klienta.
//!
//! To jest test KONTRAKTU między trzema warstwami:
//!   harness (klient) → node (koordynator+wykonawca) → z powrotem do klienta
//!
//! Sprawdza, że klient weryfikuje wynik SAM (RULES #6) — bez zaufania do koordynatora.
//! Bez sieci: nody komunikują się przez pamięć. Transport (libp2p) dochodzi osobno
//! na tych samych typach — jeśli ten test przechodzi, protokół jest spójny.




use simon_core::crypto::Keypair;
use simon_core::receipt::{Precision, Receipt};
use simon_core::SimonError;
use simon_harness::client_protocol::{
    verify_completion, JobOrder, OrderAccepted, OrderCompleted, TOPIC_JOB_ORDER,
};
use simon_node::{Capability, JobResult, RegisterMsg, Registry};

/// Pełna ścieżka: klient zleca → node wykonuje → klient weryfikuje.
struct Scena {
    client_key: Keypair,
    node_key: Keypair,
    registry: Registry,
    node_id: String,
}

impl Scena {
    fn nowa() -> Self {
        let client_key = Keypair::generate();
        let node_key = Keypair::generate();
        let mut registry = Registry::new();
        let node_id = "node-1".to_string();

        let cap = Capability {
            declared: serde_json::json!({"vram_gb": 12, "tok_s": 45}),
            // RULES #2: przydział tylko po potwierdzonej mocy.
            attested: Some(serde_json::json!({"tok_s": 42.1})),
        };
        let msg = RegisterMsg::new(node_id.clone(), node_key.public(), cap);
        assert!(registry.register(msg), "rejestracja node'a");

        Self {
            client_key,
            node_key,
            registry,
            node_id,
        }
    }

    fn zlecenie(&self, _order_id: &str, fee: u64) -> JobOrder {
        // M0.2: order_id jest LICZONY (digest + nonce), nie podawany.
        let mut o = JobOrder::new(
            "sha256:qwen3.8-27b-w4a16",
            b"policz cos",
            fee,
            300,
            self.client_key.public().to_hex(),
        )
        .expect("zlecenie");
        // Podpis klienta nad odciskiem treści.
        let digest = o.digest().expect("odcisk");
        o.signature = Some(hex::encode(
            self.client_key.sign_digest(&digest).0.to_vec(),
        ));
        o
    }

    /// Node wykonuje pracę i składa podpisany receipt.
    fn wykonaj(&mut self, order: &JobOrder) -> JobResult {
        let job_id = format!("job-{}", order.order_id);
        assert!(
            self.registry.assign(&job_id, &self.node_id),
            "przydział joba"
        );

        let receipt = Receipt {
            job_id: job_id.clone(),
            node_id: self.node_id.clone(),
            model_hash: order.model_hash.clone(),
            runtime: "vllm-0.6.3".to_string(),
            precision: Precision::Bf16,
            activation_hash: "toploc:258B".to_string(),
            output_digest: "sha256:wynik".to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            started_at_us: 100_000_000,
            finished_at_us: 100_400_000,
            signer: self.node_key.public(),
            signature: None,
        }
        .sign(&self.node_key)
        .expect("podpis receiptu");

        let result = JobResult::success(
            job_id,
            self.node_id.clone(),
            serde_json::json!({"tokens": 128}),
            receipt,
        );
        self.registry.complete(&result).expect("przyjęcie wyniku");
        result
    }

    /// Klient weryfikuje wynik SAM — klucz node'a bierze z rejestru, nie z receiptu.
    ///
    /// `wiazanie` to `order_id`, które KOORDYNATOR wpisuje do `OrderCompleted`.
    /// W prawdziwym protokole pochodzi z zewnątrz (koordynator wie, do czego
    /// przypisał job). W teście podajemy je jawnie, żeby móc sprawdzić atak.
    fn klient_weryfikuje(
        &self,
        wiazanie: &str,
        oczekiwane: &str,
        result: &JobResult,
    ) -> Result<(), SimonError> {
        let receipt = result.receipt.as_ref().ok_or(SimonError::BadSignature)?;
        let registered = self
            .registry
            .nodes
            .get(&self.node_id)
            .expect("node w rejestrze")
            .pubkey
            .clone();

        let completed = OrderCompleted {
            order_id: wiazanie.to_string(),
            job_id: receipt.job_id.clone(),
            node_id: self.node_id.clone(),
            receipt: receipt.clone(),
            coordinator_pubkey: "coord-1".to_string(),
        };

        verify_completion(&completed, oczekiwane, &registered)
    }
}

// ---------------------------------------------------------------------------
// Testy
// ---------------------------------------------------------------------------

#[test]
fn pelny_cykl_przechodzi() {
    let mut s = Scena::nowa();
    let order = s.zlecenie("o-1", 100);
    let result = s.wykonaj(&order);
    assert!(
        s.klient_weryfikuje(&order.order_id, &order.order_id, &result).is_ok(),
        "klient musi zweryfikować wynik samodzielnie"
    );
}

#[test]
fn zlecenie_bez_oplaty_nie_generuje_emisji() {
    let s = Scena::nowa();
    let order = s.zlecenie("o-2", 0);
    assert!(!order.is_funded(), "D83: fee_think == 0 → brak emisji");
}

#[test]
fn roboczy_burn_proof_nie_jest_produkcyjny() {
    let s = Scena::nowa();
    let order = s.zlecenie("o-3", 50);
    assert!(order.burn.synthetic);
    assert!(!order.has_verifiable_burn(), "MVP nie udaje produkcji");
}

#[test]
fn klient_odrzuca_wynik_dla_innego_zlecenia() {
    let mut s = Scena::nowa();
    let order = s.zlecenie("o-4", 50);
    let result = s.wykonaj(&order);

    // Wynik dotyczy o-4, ale klient czeka na o-INNE.
    // UWAGA: job_id w receipcie NIE zawiera order_id (jest nadawany przez
    // koordynatora), więc tej rozbieżności NIE da się złapać po samym receiptcie.
    // Łapie ją `order_id` na poziomie OrderCompleted — to jest cała rola tego pola.
    // Koordynator podstawia order_id, którego klient NIE zamawiał.
    assert!(
        s.klient_weryfikuje("o-INNE", &order.order_id, &result).is_err(),
        "wynik dla innego zlecenia musi być odrzucony"
    );

    // Kontrola niepustości: właściwe wiązanie MUSI przechodzić.
    // Bez niej test przechodziłby także wtedy, gdyby weryfikacja była zepsuta.
    assert!(
        s.klient_weryfikuje(&order.order_id, &order.order_id, &result).is_ok(),
        "kontrola: właściwe zlecenie musi przechodzić"
    );
}

#[test]
fn klient_odrzuca_receipt_podpisany_obcym_kluczem() {
    let mut s = Scena::nowa();
    let order = s.zlecenie("o-5", 50);

    // Node wykonuje, ale receipt podpisujemy OBCYM kluczem.
    let job_id = format!("job-{}", order.order_id);
    s.registry.assign(&job_id, &s.node_id);

    let obcy = Keypair::generate();
    let receipt = Receipt {
        job_id: job_id.clone(),
        node_id: s.node_id.clone(),
        model_hash: order.model_hash.clone(),
        runtime: "vllm-0.6.3".to_string(),
        precision: Precision::Bf16,
        activation_hash: "toploc:258B".to_string(),
        output_digest: "sha256:wynik".to_string(),
        prompt_tokens: 0,
        completion_tokens: 0,
        started_at_us: 100_000_000,
        finished_at_us: 100_400_000,
        signer: obcy.public(),
        signature: None,
    }
    .sign(&obcy)
    .unwrap();

    let result = JobResult::success(
        job_id,
        s.node_id.clone(),
        serde_json::json!({}),
        receipt,
    );

    assert!(
        s.klient_weryfikuje(&order.order_id, &order.order_id, &result).is_err(),
        "wariant C: obcy klucz musi być odrzucony"
    );
}

#[test]
fn klient_odrzuca_podmieniona_tresc_receiptu() {
    let mut s = Scena::nowa();
    let order = s.zlecenie("o-6", 50);
    let mut result = s.wykonaj(&order);

    // Podmiana po podpisaniu.
    if let Some(r) = result.receipt.as_mut() {
        r.activation_hash = "toploc:FAKE".to_string();
    }

    assert!(
        s.klient_weryfikuje(&order.order_id, &order.order_id, &result).is_err(),
        "podmiana treści receiptu musi być wykryta"
    );
}

#[test]
fn wyscig_gossipsub_rozstrzygany_deterministycznie() {
    // Sytuacja przy gossipsub: DWÓCH koordynatorów łapie to samo zlecenie.
    // Bez reguły = podwójne rozliczenie (F2).
    let mk = |pk: &str| OrderAccepted {
        order_id: "o-7".into(),
        job_id: "job-o-7".into(),
        coordinator_pubkey: pk.to_string(),
        vrf_seed: "vrf:1".into(),
        vrf_block: 42,
        estimated_ms: 200,
        signature: None,
    };

    let k1 = mk("0a1b");
    let k2 = mk("ff99");
    let k3 = mk("5c3d");
    let kandydaci = [k1, k2, k3];

    let zwyciezca = OrderAccepted::winner(&kandydaci).expect("jest zwycięzca");
    // Reguła: min H(order_id ‖ pubkey) — NIE min pubkey (P5 z Opusa).
    // Przy min pubkey klucz "0a1b" wygrywałby, a klucz "0000…" przejmowałby
    // każdy wyścig w sieci bez żadnej pracy.
    let ranking = |pk: &str| {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"o-7|");
        h.update(pk.as_bytes());
        hex::encode(h.finalize())
    };
    let oczekiwany = ["0a1b", "ff99", "5c3d"]
        .into_iter()
        .min_by_key(|pk| ranking(pk))
        .unwrap();
    assert_eq!(zwyciezca.coordinator_pubkey, oczekiwany, "wygrywa min H(order_id ‖ pubkey)");

    // Ten sam wynik niezależnie od kolejności odpowiedzi.
    for i in 0..kandydaci.len() {
        let mut obrocone = kandydaci.clone();
        obrocone.rotate_left(i);
        let w = OrderAccepted::winner(&obrocone).unwrap();
        assert_eq!(
            w.coordinator_pubkey, oczekiwany,
            "kolejność odpowiedzi nie może zmieniać zwycięzcy"
        );
    }
}

#[test]
fn tematy_gossipsub_sa_czescia_kontraktu() {
    // Zmiana tematu = nowa sieć. To jest część nieodwracalnego formatu.
    assert_eq!(TOPIC_JOB_ORDER, "simon/v1/job-order");
}

#[test]
fn porazka_nie_wymaga_receiptu() {
    let mut s = Scena::nowa();
    let job_id = "job-fail";
    s.registry.assign(job_id, &s.node_id);
    let result = JobResult::failure(job_id, s.node_id.clone(), "OOM");
    assert!(s.registry.complete(&result).is_ok());
}

// ---------------------------------------------------------------------------
// Pomocnicze: sprawdzenie, że typy serde działają (transport libp2p tego wymaga)
// ---------------------------------------------------------------------------

#[test]
fn typy_sa_serializowalne_do_json() {
    let s = Scena::nowa();
    let order = s.zlecenie("o-json", 10);

    let json = serde_json::to_string(&order).expect("JobOrder → JSON");
    let back: JobOrder = serde_json::from_str(&json).expect("JSON → JobOrder");
    assert_eq!(back.order_id, order.order_id);
    assert_eq!(back.digest().unwrap(), order.digest().unwrap());

    // Po round-trip odcisk musi być identyczny — inaczej podpis nie przetrwa sieci.
    assert_eq!(
        back.payload_digest, order.payload_digest,
        "round-trip nie może zmieniać odcisku"
    );
}
