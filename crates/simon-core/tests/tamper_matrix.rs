//! Macierz tamper (krytyk muse, 2026-10-04): „jeśli binding pada, M3/IFC/staking
//! dziedziczą forgery". Testujemy, że **każde** pole związane w receipcie v2 jest
//! objęte podpisem: zmiana jednego bitu → `verify_self()` = Err. Osobno: grant
//! (double-use/expired/zły parametr) i replay nonce.
//!
//! To licencjonuje TYLKO: „v2 receipts są tamper-evident authorship/binding pod
//! Ed25519 przy założeniu tajności klucza". NIE licencjonuje poprawności wykonania,
//! tożsamości modelu ani poufności wobec executora.

use simon_core::cap::{
    odcisk_parametrow_akcji, sprawdz, Kontekst, OdmowaZ, Zezwolenie,
};
use simon_core::crypto::Keypair;
use simon_core::protokol::{BladProtokolu, OknoNonce};
use simon_core::receipt::{odcisk_parametrow, odcisk_wyjscia, Precision, Receipt};

fn bazowy_podpisany() -> (Receipt, Keypair) {
    let k = Keypair::from_seed(&[11u8; 32]);
    let r = Receipt {
        job_id: "job-1".into(),
        node_id: k.public().to_hex(),
        model_hash: "qwen2.5-7b-instruct-q8".into(),
        runtime: "llama.cpp/511f9c13".into(),
        precision: Precision::Fp16,
        activation_hash: "toploc:NIE_POLICZONY".into(),
        output_digest: odcisk_wyjscia("job-1", "hello world").unwrap(),
        prompt_tokens: 12,
        completion_tokens: 34,
        started_at_us: 1_000_000,
        finished_at_us: 2_000_000,
        signer: k.public(),
        signature: None,
        wiazania: None,
    }
    .z_wiazaniem_m1(
        "tokhash".into(),
        "nonce-abc".into(),
        &[1, 2, 3, 4],
        &[5, 6, 7],
        odcisk_parametrow(0, 1_000_000, 1, 0, 1_000_000).unwrap(),
        42,
        "llama.cpp/Q8_0/cpu".into(),
    )
    .unwrap()
    .sign(&k)
    .unwrap();
    (r, k)
}

#[test]
fn kazde_pole_v2_jest_pod_podpisem() {
    let (bazowy, _k) = bazowy_podpisany();
    assert!(bazowy.verify_self().is_ok());
    assert_eq!(bazowy.poziom(), 1);

    // (nazwa, mutator)
    let mutatory: Vec<(&str, Box<dyn Fn(&mut Receipt)>)> = vec![
        ("job_id", Box::new(|r| r.job_id.push('x'))),
        ("node_id", Box::new(|r| r.node_id.push('x'))),
        ("model_hash", Box::new(|r| r.model_hash.push('x'))),
        ("runtime", Box::new(|r| r.runtime.push('x'))),
        ("precision", Box::new(|r| r.precision = Precision::Fp8)),
        ("activation_hash", Box::new(|r| r.activation_hash.push('x'))),
        ("output_digest", Box::new(|r| r.output_digest.push('x'))),
        ("prompt_tokens", Box::new(|r| r.prompt_tokens += 1)),
        ("completion_tokens", Box::new(|r| r.completion_tokens += 1)),
        ("started_at_us", Box::new(|r| r.started_at_us += 1)),
        ("finished_at_us", Box::new(|r| r.finished_at_us += 1)),
        ("signer", Box::new(|r| {
            r.signer = Keypair::from_seed(&[99u8; 32]).public()
        })),
        // pola M1
        ("wiazania.schema_version", Box::new(|r| {
            r.wiazania.as_mut().unwrap().schema_version += 1
        })),
        ("wiazania.receipt_level", Box::new(|r| {
            r.wiazania.as_mut().unwrap().receipt_level = 0
        })),
        ("wiazania.exec_profile", Box::new(|r| {
            r.wiazania.as_mut().unwrap().exec_profile.push('x')
        })),
        ("wiazania.tokenizer_hash", Box::new(|r| {
            r.wiazania.as_mut().unwrap().tokenizer_hash.push('x')
        })),
        ("wiazania.prompt_digest", Box::new(|r| {
            r.wiazania.as_mut().unwrap().prompt_digest.push('x')
        })),
        ("wiazania.output_token_chain", Box::new(|r| {
            r.wiazania.as_mut().unwrap().output_token_chain.push('x')
        })),
        ("wiazania.client_nonce", Box::new(|r| {
            r.wiazania.as_mut().unwrap().client_nonce.push('x')
        })),
        ("wiazania.sampling_params_hash", Box::new(|r| {
            r.wiazania.as_mut().unwrap().sampling_params_hash.push('x')
        })),
        ("wiazania.rng_seed", Box::new(|r| {
            r.wiazania.as_mut().unwrap().rng_seed += 1
        })),
    ];

    let mut przeszly = Vec::new();
    for (nazwa, mutuj) in &mutatory {
        let mut r = bazowy.clone();
        mutuj(&mut r);
        if r.verify_self().is_ok() {
            przeszly.push(*nazwa);
        }
    }
    assert!(
        przeszly.is_empty(),
        "te pola NIE są pod podpisem (tamper niewykryty): {przeszly:?}"
    );

    // podmiana samego podpisu też łamie
    let mut r = bazowy.clone();
    r.signature = Some(simon_core::crypto::Signature([0u8; 64]));
    assert!(r.verify_self().is_err(), "podmieniony podpis musi być odrzucony");
}

#[test]
fn stary_receipt_to_authorship_bez_bindingu() {
    // Legacy receipt (poziom 0) weryfikuje podpis, ale NIE niesie wiązania wejścia —
    // więc `zweryfikuj_m1` musi go odrzucić (authorship-only, nie binding).
    let r: Receipt =
        serde_json::from_str(include_str!("../../../przyklady/receipt.json").trim()).unwrap();
    assert_eq!(r.poziom(), 0);
    assert!(r.verify_self().is_ok(), "authorship legacy działa");
    assert!(
        r.zweryfikuj_m1("cokolwiek", "cokolwiek", &[1], &[2]).is_err(),
        "poziom 0 nie ma wiązania — nie wolno go przedstawiać jako binding"
    );
}

#[test]
fn grant_double_use_expired_zly_parametr() {
    let op = Keypair::from_seed(&[7u8; 32]);
    let pk = op.public().to_hex();
    let par = serde_json::json!({"msg": "ok"});
    let z = Zezwolenie {
        id: "g-1".into(),
        akcja: "log_message".into(),
        parametry_digest: odcisk_parametrow_akcji("log_message", &par).unwrap(),
        wystawca: String::new(),
        wazne_do_us: 1_000_000,
        budzet: Some(1),
        podpis: None,
    }
    .podpisz(&op)
    .unwrap();
    let sufit = simon_core::cap::Sufit {
        akcje: std::collections::BTreeMap::from([(
            "log_message".to_string(),
            simon_core::cap::Akcja {
                nazwa: "log_message".into(),
                parametry: vec![simon_core::cap::Parametr {
                    nazwa: "msg".into(),
                    wymagany: true,
                    ograniczenie: simon_core::cap::Ograniczenie::Tekst { max_len: 200 },
                }],
            },
        )]),
    };
    let uzyte = vec!["g-1".to_string()];
    let ok = Kontekst {
        teraz_us: 500_000,
        oczekiwany_wystawca: &pk,
        odwolane: &[],
        uzyte: &[],
    };
    assert!(sprawdz(&sufit, &z, "log_message", &par, &ok).is_ok());
    // double-use
    let ok_uzyte = Kontekst {
        teraz_us: 500_000,
        oczekiwany_wystawca: &pk,
        odwolane: &[],
        uzyte: &uzyte,
    };
    assert_eq!(
        sprawdz(&sufit, &z, "log_message", &par, &ok_uzyte),
        Err(OdmowaZ::Uzyte)
    );
    // zły parametr
    let zly = serde_json::json!({"msg": "rm -rf /"});
    assert_eq!(
        sprawdz(&sufit, &z, "log_message", &zly, &ok),
        Err(OdmowaZ::Parametry)
    );
    // expired
    let ok_exp = Kontekst {
        teraz_us: 2_000_000,
        oczekiwany_wystawca: &pk,
        odwolane: &[],
        uzyte: &[],
    };
    assert_eq!(
        sprawdz(&sufit, &z, "log_message", &par, &ok_exp),
        Err(OdmowaZ::Wygaslo)
    );
}

#[test]
fn replay_nonce_odrzucony() {
    let mut o = OknoNonce::nowy(16);
    assert!(o.przyjmij(1000).is_ok());
    assert_eq!(o.przyjmij(1000), Err(BladProtokolu::NoncePowtorzony));
}
