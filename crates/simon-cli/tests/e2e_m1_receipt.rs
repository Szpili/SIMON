//! Test integracyjny E2E (Track H): weryfikacja generowania i wiązania tokenów M1
//!
//! Sprawdza 4 scenariusze end-to-end:
//! 1. Uczciwy przepływ M1: node z działającym `/tokenize` generuje podpisany receipt poziomu 1,
//!    `PromptResponse` niesie `tokenizer_hash`, `prompt_token_ids` i `output_token_ids`,
//!    a klient weryfikuje receipt za pomocą `Receipt::zweryfikuj_m1` z oryginalnym `nonce`.
//! 2. Zmanipulowane identyfikatory tokenów wyjścia (`output_token_ids`): zmiana choćby 1 tokenu
//!    powoduje błąd weryfikacji M1.
//! 3. Błędny `client_nonce`: weryfikacja z innym nonce zawodzi.
//! 4. Czysty fallback do poziomu M0: gdy `/tokenize` zwraca 404, nieprawidłowe dane lub
//!    tablicę przekraczającą limit `MAX_M1_TOKENS` (65 536), node podpisuje poprawny
//!    receipt poziomu 0 (`wiazania: None`, `tokenizer_hash: None`).

use simon_core::crypto::Keypair;
use simon_core::receipt::Receipt;
use simon_harness::request_response::{PromptReply, PromptRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[path = "../src/wspolne.rs"]
pub mod wspolne;

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

/// Tryb odpowiedzi mockowanego backendu HTTP na zapytanie `/tokenize`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenizeTryb {
    /// Prawidłowa tokenizacja promptu i wyjścia
    Poprawny,
    /// Endpoint `/tokenize` nie istnieje (HTTP 404)
    BrakEndpointu404,
    /// Przekroczony limit `MAX_M1_TOKENS` (65 536)
    PonadLimit,
    /// Nieprawidłowy format tokenów (np. ciągi zamiast u32)
    NiepoprawneTokeny,
}

/// Uruchamia wewnątrzprocesowy serwer HTTP symulujący API modelu (vLLM / llama-server).
async fn mock_backend(tryb: TokenizeTryb) -> u16 {
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
                let mut przeczytano = 0;
                while przeczytano < buf.len() {
                    let n = match socket.read(&mut buf[przeczytano..]).await {
                        Ok(n) if n > 0 => n,
                        _ => break,
                    };
                    przeczytano += n;
                    if buf[..przeczytano].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_text = String::from_utf8_lossy(&buf[..przeczytano]);
                let header_line = req_text.lines().next().unwrap_or("");

                let (status, body) = if header_line.contains("/v1/models") {
                    ("200 OK", r#"{"data":[{"id":"qwen-fp16","max_model_len":4096}]}"#.to_string())
                } else if header_line.contains("/version") {
                    ("200 OK", r#"{"version":"vllm/0.6.3"}"#.to_string())
                } else if header_line.contains("/v1/chat/completions") {
                    let c = r#"{"choices":[{"message":{"role":"assistant","content":"42 to odpowiedz"}}],"usage":{"completion_tokens":3,"prompt_tokens":4}}"#;
                    ("200 OK", c.to_string())
                } else if header_line.contains("/tokenize") {
                    match tryb {
                        TokenizeTryb::Poprawny => {
                            if req_text.contains("42 to odpowiedz") {
                                ("200 OK", r#"{"tokens":[42,55,999]}"#.to_string())
                            } else {
                                ("200 OK", r#"{"tokens":[101,202,303,404]}"#.to_string())
                            }
                        }
                        TokenizeTryb::BrakEndpointu404 => {
                            ("404 Not Found", r#"{"error":"Not Found"}"#.to_string())
                        }
                        TokenizeTryb::PonadLimit => {
                            let nadmiar = vec![1u32; wspolne::MAX_M1_TOKENS + 1];
                            let c = serde_json::to_string(&serde_json::json!({ "tokens": nadmiar })).unwrap();
                            ("200 OK", c)
                        }
                        TokenizeTryb::NiepoprawneTokeny => {
                            ("200 OK", r#"{"tokens":["niepoprawny", 123]}"#.to_string())
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
    let port = mock_backend(TokenizeTryb::Poprawny).await;
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

    let reply = node::policz(
        &format!("http://127.0.0.1:{port}"),
        "qwen-fp16",
        "vllm/0.6.3",
        &klucz,
        &req,
    )
    .await;

    let resp = match reply {
        PromptReply::Ok(r) => r,
        PromptReply::Blad(e) => panic!("nieoczekiwany błąd node'a: {e:?}"),
    };

    assert_eq!(resp.output, "42 to odpowiedz");
    assert!(resp.tokenizer_hash.is_some());
    assert_eq!(resp.prompt_token_ids, Some(vec![101, 202, 303, 404]));
    assert_eq!(resp.output_token_ids, Some(vec![42, 55, 999]));

    let r: Receipt = serde_json::from_str(&resp.receipt).expect("poprawny JSON receiptu");
    assert_eq!(r.poziom(), 1, "receipt musi posiadać poziom M1");
    assert!(r.wiazania.is_some(), "wiazania M1 muszą być obecne");
    assert!(r.verify_self().is_ok(), "podpis node'a musi być prawidłowy");
    assert!(r.zgodny_z_wyjsciem(&resp.output), "receipt musi pokrywać treść wyjścia");

    // Weryfikacja klienta z oryginalnym nonce zlecenia
    let th = resp.tokenizer_hash.as_ref().unwrap();
    let pt = resp.prompt_token_ids.as_ref().unwrap();
    let ot = resp.output_token_ids.as_ref().unwrap();
    assert!(
        r.zweryfikuj_m1(th, &req.nonce, pt, ot).is_ok(),
        "weryfikacja M1 klienta musi przejść pomyślnie"
    );
}

#[tokio::test]
async fn test_e2e_scenariusz_2_zmanipulowane_tokeny_wyjscia_odrzucone() {
    let port = mock_backend(TokenizeTryb::Poprawny).await;
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

    let reply = node::policz(
        &format!("http://127.0.0.1:{port}"),
        "qwen-fp16",
        "vllm/0.6.3",
        &klucz,
        &req,
    )
    .await;

    let resp = match reply {
        PromptReply::Ok(r) => r,
        PromptReply::Blad(e) => panic!("błąd node'a: {e:?}"),
    };

    let r: Receipt = serde_json::from_str(&resp.receipt).expect("parsowanie receiptu");
    let th = resp.tokenizer_hash.as_ref().unwrap();
    let pt = resp.prompt_token_ids.as_ref().unwrap();
    let mut zmanipulowane_ot = resp.output_token_ids.clone().unwrap();

    // Odwracamy 1 bit w tokenie wyjściowym
    zmanipulowane_ot[0] ^= 1;

    let wynik = r.zweryfikuj_m1(th, &req.nonce, pt, &zmanipulowane_ot);
    assert!(
        wynik.is_err(),
        "weryfikacja M1 ze zmanipulowanymi tokenami wyjścia MUSI zwrócić błąd"
    );
}

#[tokio::test]
async fn test_e2e_scenariusz_3_zly_client_nonce_odrzucony() {
    let port = mock_backend(TokenizeTryb::Poprawny).await;
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

    let reply = node::policz(
        &format!("http://127.0.0.1:{port}"),
        "qwen-fp16",
        "vllm/0.6.3",
        &klucz,
        &req,
    )
    .await;

    let resp = match reply {
        PromptReply::Ok(r) => r,
        PromptReply::Blad(e) => panic!("błąd node'a: {e:?}"),
    };

    let r: Receipt = serde_json::from_str(&resp.receipt).expect("parsowanie receiptu");
    let th = resp.tokenizer_hash.as_ref().unwrap();
    let pt = resp.prompt_token_ids.as_ref().unwrap();
    let ot = resp.output_token_ids.as_ref().unwrap();

    let wynik = r.zweryfikuj_m1(th, "inny-sfalszowany-nonce", pt, ot);
    assert!(
        wynik.is_err(),
        "weryfikacja M1 z niezgodnym nonce klienta MUSI zwrócić błąd"
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

    // 4a: Endpoint /tokenize nie istnieje (HTTP 404)
    {
        let port_404 = mock_backend(TokenizeTryb::BrakEndpointu404).await;
        let reply = node::policz(
            &format!("http://127.0.0.1:{port_404}"),
            "qwen-fp16",
            "vllm/0.6.3",
            &klucz,
            &req,
        )
        .await;

        let resp = match reply {
            PromptReply::Ok(r) => r,
            PromptReply::Blad(e) => panic!("błąd node'a przy braku /tokenize: {e:?}"),
        };

        assert_eq!(resp.tokenizer_hash, None);
        assert_eq!(resp.prompt_token_ids, None);
        assert_eq!(resp.output_token_ids, None);

        let r: Receipt = serde_json::from_str(&resp.receipt).expect("parsowanie receiptu");
        assert_eq!(r.poziom(), 0, "przy braku /tokenize poziom receiptu musi wynosić 0");
        assert!(r.wiazania.is_none());
        assert!(r.verify_self().is_ok());
        assert!(r.zgodny_z_wyjsciem(&resp.output));
    }

    // 4b: /tokenize zwraca ponad MAX_M1_TOKENS tokenów
    {
        let port_overcap = mock_backend(TokenizeTryb::PonadLimit).await;
        let reply = node::policz(
            &format!("http://127.0.0.1:{port_overcap}"),
            "qwen-fp16",
            "vllm/0.6.3",
            &klucz,
            &req,
        )
        .await;

        let resp = match reply {
            PromptReply::Ok(r) => r,
            PromptReply::Blad(e) => panic!("błąd node'a przy overcap: {e:?}"),
        };

        assert_eq!(resp.tokenizer_hash, None);
        assert_eq!(resp.prompt_token_ids, None);
        assert_eq!(resp.output_token_ids, None);

        let r: Receipt = serde_json::from_str(&resp.receipt).expect("parsowanie receiptu");
        assert_eq!(r.poziom(), 0, "przy overcap poziom receiptu musi wynosić 0");
        assert!(r.wiazania.is_none());
        assert!(r.verify_self().is_ok());
        assert!(r.zgodny_z_wyjsciem(&resp.output));
    }

    // 4c: /tokenize zwraca niepoprawny format tablicy tokenów
    {
        let port_invalid = mock_backend(TokenizeTryb::NiepoprawneTokeny).await;
        let reply = node::policz(
            &format!("http://127.0.0.1:{port_invalid}"),
            "qwen-fp16",
            "vllm/0.6.3",
            &klucz,
            &req,
        )
        .await;

        let resp = match reply {
            PromptReply::Ok(r) => r,
            PromptReply::Blad(e) => panic!("błąd node'a przy niepoprawnych tokenach: {e:?}"),
        };

        assert_eq!(resp.tokenizer_hash, None);
        assert_eq!(resp.prompt_token_ids, None);
        assert_eq!(resp.output_token_ids, None);

        let r: Receipt = serde_json::from_str(&resp.receipt).expect("parsowanie receiptu");
        assert_eq!(r.poziom(), 0);
        assert!(r.wiazania.is_none());
        assert!(r.verify_self().is_ok());
    }

    // 4d: Tryb czatu (req.chat = true) -> czysty fallback do poziomu 0
    {
        let port_chat = mock_backend(TokenizeTryb::Poprawny).await;
        let mut chat_req = req.clone();
        chat_req.chat = true;
        chat_req.prompt = r#"{"messages":[{"role":"user","content":"hej"}]}"#.into();

        let reply = node::policz(
            &format!("http://127.0.0.1:{port_chat}"),
            "qwen-fp16",
            "vllm/0.6.3",
            &klucz,
            &chat_req,
        )
        .await;

        let resp = match reply {
            PromptReply::Ok(r) => r,
            PromptReply::Blad(e) => panic!("błąd node'a w trybie czatu: {e:?}"),
        };

        assert_eq!(resp.tokenizer_hash, None);
        assert_eq!(resp.prompt_token_ids, None);
        assert_eq!(resp.output_token_ids, None);

        let r: Receipt = serde_json::from_str(&resp.receipt).expect("parsowanie receiptu");
        assert_eq!(r.poziom(), 0);
        assert!(r.wiazania.is_none());
        assert!(r.verify_self().is_ok());
    }
}
