//! Macierz tamper (krytyk muse, 2026-10-04): „jeśli binding pada, M3/IFC/staking
//! dziedziczą forgery". Testujemy, że **każde** pole związane w receipcie v2 jest
//! objęte podpisem: zmiana jednego bitu → `verify_self()` = Err. Osobno: grant
//! (double-use/expired/zły parametr) i replay nonce.
//!
//! To licencjonuje TYLKO: „v2 receipts są tamper-evident authorship/binding pod
//! Ed25519 przy założeniu tajności klucza". NIE licencjonuje poprawności wykonania,
//! tożsamości modelu ani poufności wobec executora.

use simon_core::crypto::Keypair;
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
        bindings: None,
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
    type Mutator = Box<dyn Fn(&mut Receipt)>;
    let mutatory: Vec<(&str, Mutator)> = vec![
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
        ("bindings.schema_version", Box::new(|r| {
            r.bindings.as_mut().unwrap().schema_version += 1
        })),
        ("bindings.receipt_level", Box::new(|r| {
            r.bindings.as_mut().unwrap().receipt_level = 0
        })),
        ("bindings.exec_profile", Box::new(|r| {
            r.bindings.as_mut().unwrap().exec_profile.push('x')
        })),
        ("bindings.tokenizer_hash", Box::new(|r| {
            r.bindings.as_mut().unwrap().tokenizer_hash.push('x')
        })),
        ("bindings.prompt_digest", Box::new(|r| {
            r.bindings.as_mut().unwrap().prompt_digest.push('x')
        })),
        ("bindings.output_token_chain", Box::new(|r| {
            r.bindings.as_mut().unwrap().output_token_chain.push('x')
        })),
        ("bindings.client_nonce", Box::new(|r| {
            r.bindings.as_mut().unwrap().client_nonce.push('x')
        })),
        ("bindings.sampling_params_hash", Box::new(|r| {
            r.bindings.as_mut().unwrap().sampling_params_hash.push('x')
        })),
        ("bindings.rng_seed", Box::new(|r| {
            r.bindings.as_mut().unwrap().rng_seed += 1
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
fn zweryfikuj_m1_odrzuca_niepodpisany_receipt() {
    let (bazowy, _) = bazowy_podpisany();
    let mut unsigned = bazowy.clone();
    unsigned.signature = None;
    assert!(
        unsigned
            .zweryfikuj_m1("tokhash", "nonce-abc", &[1, 2, 3, 4], &[5, 6, 7])
            .is_err(),
        "niepodpisany receipt nie może przejść zweryfikuj_m1"
    );
}

#[test]
fn grant_double_use_expired_zly_parametr() {
    let op = Keypair::from_seed(&[7u8; 32]);
    let pk = op.public().to_hex();
    let par = serde_json::json!({"msg": "ok"});
    let z = simon_core::cap::CapabilityGrant {
        id: "g-1".into(),
        action: "log_message".into(),
        params_digest: simon_core::cap::action_params_digest("log_message", &par).unwrap(),
        issuer: String::new(),
        valid_until_us: 1_000_000,
        budget: Some(1),
        signature: None,
    }
    .sign(&op)
    .unwrap();
    let sufit = simon_core::cap::CapabilityCeiling {
        actions: std::collections::BTreeMap::from([(
            "log_message".to_string(),
            simon_core::cap::ActionDef {
                name: "log_message".into(),
                params: vec![simon_core::cap::ActionParam {
                    name: "msg".into(),
                    required: true,
                    constraint: simon_core::cap::ParamConstraint::Text { max_len: 200 },
                }],
            },
        )]),
    };
    let uzyte = vec!["g-1".to_string()];
    let ok = simon_core::cap::GrantContext {
        now_us: 500_000,
        expected_issuer: &pk,
        revoked: &[],
        used: &[],
    };
    assert!(simon_core::cap::verify_grant(&sufit, &z, "log_message", &par, &ok).is_ok());
    // double-use
    let ok_uzyte = simon_core::cap::GrantContext {
        now_us: 500_000,
        expected_issuer: &pk,
        revoked: &[],
        used: &uzyte,
    };
    assert_eq!(
        simon_core::cap::verify_grant(&sufit, &z, "log_message", &par, &ok_uzyte),
        Err(simon_core::cap::GrantRejection::Used)
    );
    // zły parametr
    let zly = serde_json::json!({"msg": "rm -rf /"});
    assert_eq!(
        simon_core::cap::verify_grant(&sufit, &z, "log_message", &zly, &ok),
        Err(simon_core::cap::GrantRejection::Params)
    );
    // expired
    let ok_exp = simon_core::cap::GrantContext {
        now_us: 2_000_000,
        expected_issuer: &pk,
        revoked: &[],
        used: &[],
    };
    assert_eq!(
        simon_core::cap::verify_grant(&sufit, &z, "log_message", &par, &ok_exp),
        Err(simon_core::cap::GrantRejection::Expired)
    );
}

#[test]
fn replay_nonce_odrzucony() {
    let mut o = simon_core::protocol::NonceWindow::new(16);
    assert!(o.accept(1000).is_ok());
    assert_eq!(o.accept(1000), Err(simon_core::protocol::ProtocolError::ReplayNonce));
}
