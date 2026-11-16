//! M1 fixture generator for `przyklady/` — receipt with input/state binding
//! and job task file (`--tokens`) for `simon verify`.
//!
//! Run from repo root: `cargo run -p simon-core --example gen_m1`
use simon_core::crypto::Keypair;
use simon_core::receipt::{
    output_digest_for, params_digest, tokenizer_digest, Precision, Receipt,
};

fn main() {
    // Deterministic demo keypair for test fixtures
    let k = Keypair::from_seed(&[42u8; 32]);
    let tokenizer_hash = tokenizer_digest(b"{\"demo\":\"tokenizer\"}");
    let nonce = "demo-nonce-2026-10-04";
    let prompt_token_ids: Vec<u32> = (100..140).collect();
    let output_token_ids: Vec<u32> = (200..260).collect();
    let sampling = params_digest(0, 1000, 1, 0, 1000).unwrap();
    let output_text = "DEMO OUTPUT";

    let receipt = Receipt {
        job_id: "job-m1-demo".into(),
        node_id: k.public().to_hex(),
        model_hash: "mistral-7b-instruct-v0.2.Q4_K_M".into(),
        runtime: "llama.cpp/511f9c13".into(),
        precision: Precision::Fp16,
        activation_hash: "toploc:NIE_POLICZONY".into(),
        output_digest: output_digest_for("job-m1-demo", output_text).unwrap(),
        prompt_tokens: prompt_token_ids.len() as u32,
        completion_tokens: output_token_ids.len() as u32,
        started_at_us: 0,
        finished_at_us: 1,
        signer: k.public(),
        signature: None,
        bindings: None,
    }
    .with_m1_bindings(
        tokenizer_hash.clone(),
        nonce.into(),
        &prompt_token_ids,
        &output_token_ids,
        sampling,
        42,
        "llama.cpp/Q4_K_M/cpu".into(),
    )
    .unwrap()
    .sign(&k)
    .unwrap();

    let task = serde_json::json!({
        "tokenizer_hash": tokenizer_hash,
        "client_nonce": nonce,
        "prompt_token_ids": prompt_token_ids,
        "output_token_ids": output_token_ids,
    });

    std::fs::write(
        "przyklady/receipt_m1.json",
        serde_json::to_string_pretty(&receipt).unwrap() + "\n",
    )
    .unwrap();
    std::fs::write("przyklady/odpowiedz_m1.txt", output_text).unwrap();
    std::fs::write(
        "przyklady/zadanie_m1.json",
        serde_json::to_string_pretty(&task).unwrap() + "\n",
    )
    .unwrap();
    println!("wrote przyklady/{{receipt_m1.json, odpowiedz_m1.txt, zadanie_m1.json}}");
}
