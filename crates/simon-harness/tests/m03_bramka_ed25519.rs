//! M0.3 — Ed25519 verification in `SignatureGate`.

use simon_core::crypto::Keypair;
use simon_harness::acp_server::{AuthorizationGate, SessionId, SignatureGate, UserOrderProposal};

fn proposal(pubkey: &str) -> UserOrderProposal {
    UserOrderProposal {
        session_id: SessionId("s1".into()),
        content: "compute something".into(),
        model_hash: "sha256:model".into(),
        fee_think: 100,
        timeout_secs: 300,
        user_pubkey: pubkey.to_string(),
    }
}

#[test]
fn m03_byle_jaki_string_nie_jest_podpisem() {
    let k = Keypair::generate();
    let z = proposal(&k.public().to_hex());
    let gate = SignatureGate;
    assert!(
        gate.authorize(&z, "not-a-signature-just-text").is_err(),
        "D110: arbitrary string must not pass as signature"
    );
    let junk = "ab".repeat(64);
    assert!(
        gate.authorize(&z, &junk).is_err(),
        "D110: 64-byte junk must not pass as signature"
    );
}

#[test]
fn m03_prawdziwy_podpis_przechodzi() {
    let k = Keypair::generate();
    let z = proposal(&k.public().to_hex());
    let digest = z.digest().expect("digest");
    let sig = hex::encode(k.sign_digest(&digest).0);
    let gate = SignatureGate;
    assert!(gate.authorize(&z, &sig).is_ok(), "valid signature must pass");
}

#[test]
fn m03_podpis_obcym_kluczem_odrzucony() {
    let k = Keypair::generate();
    let foreign = Keypair::generate();
    let z = proposal(&k.public().to_hex());
    let digest = z.digest().expect("digest");
    let sig = hex::encode(foreign.sign_digest(&digest).0);
    let gate = SignatureGate;
    assert!(
        gate.authorize(&z, &sig).is_err(),
        "signature from foreign key must be rejected"
    );
}

#[test]
fn m03_podpis_nad_inna_trescia_odrzucony() {
    let k = Keypair::generate();
    let z = proposal(&k.public().to_hex());
    let digest = z.digest().expect("digest");
    let sig = hex::encode(k.sign_digest(&digest).0);

    let mut tampered = z.clone();
    tampered.content = "TRANSFER ALL THINK TO X".into();
    let gate = SignatureGate;
    assert!(
        gate.authorize(&tampered, &sig).is_err(),
        "signature over different content must be rejected"
    );

    let mut pricier = proposal(&k.public().to_hex());
    pricier.fee_think = 999_999;
    assert!(gate.authorize(&pricier, &sig).is_err(), "fee tampering rejected");
}

#[test]
fn m03_zly_format_podpisu() {
    let k = Keypair::generate();
    let z = proposal(&k.public().to_hex());
    let gate = SignatureGate;
    assert!(gate.authorize(&z, "not-hex!!").is_err());
    assert!(gate.authorize(&z, "abcd").is_err(), "too short signature");
    let mut bad_key = proposal(&k.public().to_hex());
    bad_key.user_pubkey = "zzz".into();
    assert!(gate.authorize(&bad_key, &"ab".repeat(64)).is_err());
}
