//! E2E integration test (Track H): M1 token generation and binding verification.
//!
//! Verifies 4 end-to-end scenarios:
//! 1. Honest M1 flow: node with working `/tokenize` generates signed level 1 receipt,
//!    `PromptResponse` carries `tokenizer_hash`, `prompt_token_ids` and `output_token_ids`,
//!    and client verifies receipt using `Receipt::verify_m1` with original `nonce`.
//! 2. Tampered output token IDs (`output_token_ids`): altering even 1 token
//!    causes M1 verification failure.
//! 3. Invalid `client_nonce`: verification with a mismatched nonce fails.
//! 4. Clean fallback to M0 level: when `/tokenize` returns 404, invalid data, or
//!    an array exceeding `MAX_M1_TOKENS` (65,536), node signs valid
//!    level 0 receipt (`bindings: None`, `tokenizer_hash: None`).

use simon_core::crypto::Keypair;
use simon_core::receipt::Receipt;
use simon_harness::request_response::{PromptReply, PromptRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[path = "../src/common.rs"]
pub mod common;

#[path = "../src/node.rs"]
pub mod node;

#[derive(Debug, Clone)]
pub struct Opcje {
    pub rola: Rola,
    pub listen: Option<String>,
    pub bootstrap: Vec<String>,
    pub model_url: Option<String>,
    pub model_hash: Option<String>,
    pub key: Option<String>,
    pub key_file: Option<String>,
    pub max_jobs: usize,
    pub rate_per_peer: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rola {
    Node,
}

impl Opcje {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut key = None;
        let mut listen = None;
        let mut model_url = None;
        let mut max_jobs = 1;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--key" => {
                    i += 1;
                    key = args.get(i).cloned();
                }
                "--listen" => {
                    i += 1;
                    listen = args.get(i).cloned();
                }
                "--model-url" => {
                    i += 1;
                    model_url = args.get(i).cloned();
                }
                "--max-jobs" => {
                    i += 1;
                    max_jobs = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(1);
                }
                _ => {}
            }
            i += 1;
        }
        Ok(Self {
            rola: Rola::Node,
            listen,
            bootstrap: vec![],
            model_url,
            model_hash: None,
            key,
            key_file: None,
            max_jobs,
            rate_per_peer: 0.0,
        })
    }
}

/// Response mode of mocked HTTP backend for `/tokenize`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenizeMode {
    /// Valid tokenization of prompt and output
    Valid,
    /// Endpoint `/tokenize` does not exist (HTTP 404)
    MissingEndpoint404,
    /// Exceeded limit `MAX_M1_TOKENS` (65,536)
    OverLimit,
    /// Invalid token format (e.g. strings instead of u32)
    InvalidTokens,
}

/// Runs in-process HTTP server simulating model backend API (vLLM / llama-server).
async fn mock_backend(mode: TokenizeMode) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                let mut bytes_read = 0;
                while bytes_read < buf.len() {
                    let n = match socket.read(&mut buf[bytes_read..]).await {
                        Ok(n) if n > 0 => n,
                        _ => break,
                    };
                    bytes_read += n;
                    if buf[..bytes_read].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_text = String::from_utf8_lossy(&buf[..bytes_read]);
                let header_line = req_text.lines().next().unwrap_or("");

                let (status, body) = if header_line.contains("/v1/models") {
                    ("200 OK", r#"{"data":[{"id":"qwen-fp16","max_model_len":4096}]}"#.to_string())
                } else if header_line.contains("/version") {
                    ("200 OK", r#"{"version":"vllm/0.6.3"}"#.to_string())
                } else if header_line.contains("/v1/chat/completions") {
                    let c = r#"{"choices":[{"message":{"role":"assistant","content":"42 to odpowiedz"}}],"usage":{"completion_tokens":3,"prompt_tokens":4}}"#;
                    ("200 OK", c.to_string())
                } else if header_line.contains("/tokenize") {
                    match mode {
                        TokenizeMode::Valid => {
                            if req_text.contains("42 to odpowiedz") {
                                ("200 OK", r#"{"tokens":[42,55,999]}"#.to_string())
                            } else {
                                ("200 OK", r#"{"tokens":[101,202,303,404]}"#.to_string())
                            }
                        }
                        TokenizeMode::MissingEndpoint404 => {
                            ("404 Not Found", r#"{"error":"Not Found"}"#.to_string())
                        }
                        TokenizeMode::OverLimit => {
                            let nadmiar = vec![1u32; common::MAX_M1_TOKENS + 1];
                            let c = serde_json::to_string(&serde_json::json!({ "tokens": nadmiar })).unwrap();
                            ("200 OK", c)
                        }
                        TokenizeMode::InvalidTokens => {
                            ("200 OK", r#"{"tokens":["invalid", 123]}"#.to_string())
                        }
                    }
                } else {
                    ("404 Not Found", "{}".to_string())
                };

                let resp = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });

    port
}

#[tokio::test]
async fn test_e2e_scenariusz_1_uczciwy_przeplyw_m1() {
    let port = mock_backend(TokenizeMode::Valid).await;
    let klucz = Keypair::generate();
    let req = PromptRequest {
        v: 1,
        order_id: "ord-honest-1".into(),
        job_id: "job-honest-1".into(),
        model_hash: "qwen-fp16".into(),
        prompt: "Jaka jest odpowiedz na sens zycia?".into(),
        max_tokens: 32,
        max_ctx: 1024,
        nonce: "client-nonce-777".into(),
        chat: false,
    };

    let reply = node::compute_completion(
        &format!("http://127.0.0.1:{port}"),
        "qwen-fp16",
        "vllm/0.6.3",
        &klucz,
        &req,
    )
    .await;

    let resp = match reply {
        PromptReply::Ok(r) => r,
        PromptReply::Error(e) => panic!("unexpected node error: {e:?}"),
    };

    assert_eq!(resp.output, "42 to odpowiedz");
    assert!(resp.tokenizer_hash.is_some());
    assert_eq!(resp.prompt_token_ids, Some(vec![101, 202, 303, 404]));
    assert_eq!(resp.output_token_ids, Some(vec![42, 55, 999]));

    let r: Receipt = serde_json::from_str(&resp.receipt).expect("valid receipt JSON");
    assert_eq!(r.level(), 1, "receipt must have M1 level");
    assert!(r.bindings.is_some(), "M1 bindings must be present");
    assert!(r.verify_self().is_ok(), "node signature must be valid");
    assert!(r.matches_output(&resp.output), "receipt must cover output content");

    // Client verification with original order nonce
    let th = resp.tokenizer_hash.as_ref().unwrap();
    let pt = resp.prompt_token_ids.as_ref().unwrap();
    let ot = resp.output_token_ids.as_ref().unwrap();
    assert!(
        r.verify_m1(th, &req.nonce, pt, ot).is_ok(),
        "client M1 verification must succeed"
    );
}

#[tokio::test]
async fn test_e2e_scenariusz_2_zmanipulowane_tokeny_wyjscia_odrzucone() {
    let port = mock_backend(TokenizeMode::Valid).await;
    let klucz = Keypair::generate();
    let req = PromptRequest {
        v: 1,
        order_id: "ord-tamper-out".into(),
        job_id: "job-tamper-out".into(),
        model_hash: "qwen-fp16".into(),
        prompt: "Jaka jest odpowiedz na sens zycia?".into(),
        max_tokens: 32,
        max_ctx: 1024,
        nonce: "client-nonce-888".into(),
        chat: false,
    };

    let reply = node::compute_completion(
        &format!("http://127.0.0.1:{port}"),
        "qwen-fp16",
        "vllm/0.6.3",
        &klucz,
        &req,
    )
    .await;

    let resp = match reply {
        PromptReply::Ok(r) => r,
        PromptReply::Error(e) => panic!("node error: {e:?}"),
    };

    let r: Receipt = serde_json::from_str(&resp.receipt).expect("receipt parsing");
    let th = resp.tokenizer_hash.as_ref().unwrap();
    let pt = resp.prompt_token_ids.as_ref().unwrap();
    let mut tampered_ot = resp.output_token_ids.clone().unwrap();

    // Flip 1 bit in output token
    tampered_ot[0] ^= 1;

    let result = r.verify_m1(th, &req.nonce, pt, &tampered_ot);
    assert!(
        result.is_err(),
        "M1 verification with tampered output tokens MUST return error"
    );
}

#[tokio::test]
async fn test_e2e_scenariusz_3_zly_client_nonce_odrzucony() {
    let port = mock_backend(TokenizeMode::Valid).await;
    let klucz = Keypair::generate();
    let req = PromptRequest {
        v: 1,
        order_id: "ord-tamper-nonce".into(),
        job_id: "job-tamper-nonce".into(),
        model_hash: "qwen-fp16".into(),
        prompt: "Jaka jest odpowiedz na sens zycia?".into(),
        max_tokens: 32,
        max_ctx: 1024,
        nonce: "prawidlowy-client-nonce".into(),
        chat: false,
    };

    let reply = node::compute_completion(
        &format!("http://127.0.0.1:{port}"),
        "qwen-fp16",
        "vllm/0.6.3",
        &klucz,
        &req,
    )
    .await;

    let resp = match reply {
        PromptReply::Ok(r) => r,
        PromptReply::Error(e) => panic!("node error: {e:?}"),
    };

    let r: Receipt = serde_json::from_str(&resp.receipt).expect("receipt parsing");
    let th = resp.tokenizer_hash.as_ref().unwrap();
    let pt = resp.prompt_token_ids.as_ref().unwrap();
    let ot = resp.output_token_ids.as_ref().unwrap();

    let result = r.verify_m1(th, "mismatched-forged-nonce", pt, ot);
    assert!(
        result.is_err(),
        "M1 verification with mismatched client nonce MUST return error"
    );
}

#[tokio::test]
async fn test_e2e_scenariusz_4_fallback_do_m0_gdy_tokenize_niedostepny_lub_ponad_limit() {
    let klucz = Keypair::generate();
    let req = PromptRequest {
        v: 1,
        order_id: "ord-fallback-1".into(),
        job_id: "job-fallback-1".into(),
        model_hash: "qwen-fp16".into(),
        prompt: "Sens zycia".into(),
        max_tokens: 32,
        max_ctx: 1024,
        nonce: "client-nonce-000".into(),
        chat: false,
    };

    // 4a: Endpoint /tokenize does not exist (HTTP 404)
    {
        let port_404 = mock_backend(TokenizeMode::MissingEndpoint404).await;
        let reply = node::compute_completion(
            &format!("http://127.0.0.1:{port_404}"),
            "qwen-fp16",
            "vllm/0.6.3",
            &klucz,
            &req,
        )
        .await;

        let resp = match reply {
            PromptReply::Ok(r) => r,
            PromptReply::Error(e) => panic!("node error on missing /tokenize: {e:?}"),
        };

        assert_eq!(resp.tokenizer_hash, None);
        assert_eq!(resp.prompt_token_ids, None);
        assert_eq!(resp.output_token_ids, None);

        let r: Receipt = serde_json::from_str(&resp.receipt).expect("receipt parsing");
        assert_eq!(r.level(), 0, "on missing /tokenize receipt level must be 0");
        assert!(r.bindings.is_none());
        assert!(r.verify_self().is_ok());
        assert!(r.matches_output(&resp.output));
    }

    // 4b: /tokenize returns over MAX_M1_TOKENS tokens
    {
        let port_overcap = mock_backend(TokenizeMode::OverLimit).await;
        let reply = node::compute_completion(
            &format!("http://127.0.0.1:{port_overcap}"),
            "qwen-fp16",
            "vllm/0.6.3",
            &klucz,
            &req,
        )
        .await;

        let resp = match reply {
            PromptReply::Ok(r) => r,
            PromptReply::Error(e) => panic!("node error on overcap: {e:?}"),
        };

        assert_eq!(resp.tokenizer_hash, None);
        assert_eq!(resp.prompt_token_ids, None);
        assert_eq!(resp.output_token_ids, None);

        let r: Receipt = serde_json::from_str(&resp.receipt).expect("receipt parsing");
        assert_eq!(r.level(), 0, "on overcap receipt level must be 0");
        assert!(r.bindings.is_none());
        assert!(r.verify_self().is_ok());
        assert!(r.matches_output(&resp.output));
    }

    // 4c: /tokenize returns invalid token array format
    {
        let port_invalid = mock_backend(TokenizeMode::InvalidTokens).await;
        let reply = node::compute_completion(
            &format!("http://127.0.0.1:{port_invalid}"),
            "qwen-fp16",
            "vllm/0.6.3",
            &klucz,
            &req,
        )
        .await;

        let resp = match reply {
            PromptReply::Ok(r) => r,
            PromptReply::Error(e) => panic!("node error on invalid tokens: {e:?}"),
        };

        assert_eq!(resp.tokenizer_hash, None);
        assert_eq!(resp.prompt_token_ids, None);
        assert_eq!(resp.output_token_ids, None);

        let r: Receipt = serde_json::from_str(&resp.receipt).expect("receipt parsing");
        assert_eq!(r.level(), 0);
        assert!(r.bindings.is_none());
        assert!(r.verify_self().is_ok());
    }

    // 4d: Chat mode (req.chat = true) -> clean fallback to level 0
    {
        let port_chat = mock_backend(TokenizeMode::Valid).await;
        let mut chat_req = req.clone();
        chat_req.chat = true;
        chat_req.prompt = r#"{"messages":[{"role":"user","content":"hej"}]}"#.into();

        let reply = node::compute_completion(
            &format!("http://127.0.0.1:{port_chat}"),
            "qwen-fp16",
            "vllm/0.6.3",
            &klucz,
            &chat_req,
        )
        .await;

        let resp = match reply {
            PromptReply::Ok(r) => r,
            PromptReply::Error(e) => panic!("node error in chat mode: {e:?}"),
        };

        assert_eq!(resp.tokenizer_hash, None);
        assert_eq!(resp.prompt_token_ids, None);
        assert_eq!(resp.output_token_ids, None);

        let r: Receipt = serde_json::from_str(&resp.receipt).expect("receipt parsing");
        assert_eq!(r.level(), 0);
        assert!(r.bindings.is_none());
        assert!(r.verify_self().is_ok());
    }
}
