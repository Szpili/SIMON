//! Role `node` - worker: subscribes to prompt channel, runs model, returns result + receipt.
//!
//! **M1.2:** node receives prompt via point-to-point channel (`/simon/prompt/1`),
//! not via gossipsub. Gossipsub carries only metadata without payload.

use crate::common::{build_swarm, print_listen_addrs, SwarmBehaviourEvent};
use crate::Opcje;
use futures::StreamExt;
use libp2p::request_response::{Event as RrEvent, Message as RrMessage};
use libp2p::swarm::SwarmEvent;
use simon_core::crypto::Keypair;
use simon_core::receipt::{Precision, Receipt};
use simon_harness::request_response::{PromptError, PromptReply, PromptRequest, PromptResponse};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;

/// Domniemany endpoint modelu (vLLM/Ollama, API OpenAI).
const DOMYSLNY_MODEL_URL: &str = "http://127.0.0.1:18020";

pub async fn run(opcje: &Opcje) -> Result<(), String> {
    let listen = opcje
        .listen
        .clone()
        .map(|l| vec![l])
        .unwrap_or_else(|| vec!["/ip4/0.0.0.0/tcp/9001".to_string()]);

    // Random peer_id per process to avoid collision across multiple nodes per host.
    // M2.5.1 + demo publiczne: ziarno musi byc znane PRZED budowa swarmu,
    // bo z niego wyprowadzamy takze trwaly peer_id.
    let z_pliku = match opcje.key_file.as_deref() {
        Some(p) => Some(
            std::fs::read_to_string(p)
                .map_err(|e| format!("cannot read --key-file {p}: {e}"))?
                .trim()
                .to_string(),
        ),
        None => None,
    };
    let ziarno: Option<[u8; 32]> = match z_pliku.as_deref().or(opcje.key.as_deref()) {
        Some(hexstr) => {
            let raw = hex::decode(hexstr).map_err(|e| format!("invalid --key (hex): {e}"))?;
            Some(raw.try_into()
                .map_err(|_| "--key must be 32 bytes (64 hex characters)".to_string())?)
        }
        None => None,
    };
    let mut swarm = build_swarm(&listen, &opcje.bootstrap, ziarno)?;
    print_listen_addrs(&swarm, "node");

    // C-gate: node announces itself in network (model + max_ctx) for coordinator routing.
    let topic_rejestracji = libp2p::gossipsub::IdentTopic::new(
        simon_harness::client_protocol::TOPIC_NODE_REGISTER,
    );
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic_rejestracji)
        .map_err(|e| format!("subscribe {e}"))?;

    let model_url = opcje
        .model_url
        .clone()
        .unwrap_or_else(|| DOMYSLNY_MODEL_URL.to_string());
    let model_hash = opcje
        .model_hash
        .clone()
        .unwrap_or_else(|| "nieznany".to_string());
    // C: node reads its own facts from vLLM instead of guessing.
    let (model_z_backendu, max_ctx) =
        probe_backend_facts(&model_url, opcje.model_hash.as_deref()).await;
    if max_ctx == 0 {
        eprintln!("[node] WARNING: cannot read max_model_len - will not reject by context limit");
    } else {
        println!("[node] max_ctx (z /v1/models) = {max_ctx}");
    }
    // C-gate: node declared capability (sent to registry for routing).
    // Confirmed model ID is announced if verified by backend.
    let model_deklarowany = match model_z_backendu {
        Some(m) => m,
        None => {
            eprintln!(
                "[node] WARNING: announcing '{model_hash}' WITHOUT backend confirmation"
            );
            model_hash.clone()
        }
    };
    println!("[node] declaring: model={model_deklarowany} max_ctx={max_ctx}");
    let runtime = detect_runtime(&model_url).await;
    println!("[node] model_url={model_url} model_hash={model_hash} runtime={runtime}");

    // M2.5.1: node identity. Keypair bounds output to a specific node.
    let klucz = match ziarno {
        Some(seed) => Keypair::from_seed(&seed),
        None => {
            println!("[node] WARNING: missing --key - random key generated, identity will change upon restart");
            Keypair::generate()
        }
    };
    println!("[node] node_id={}", klucz.public().to_hex());
    // Registration: actual node configuration signed with node key.
    let rejestracja = simon_node::RegisterMsg::new(
        klucz.public().to_hex(),
        klucz.public(),
        simon_node::Capability {
            declared: serde_json::json!({
                "model_hash": model_deklarowany,
                "max_ctx": max_ctx,
            }),
            // Attestation by coordinator comes in M2.5.3 (dual-card measurement).
            attested: Some(serde_json::json!({
                "model_hash": model_deklarowany,
                "max_ctx": max_ctx,
            })),
        },
    )
    .sign(&klucz)
    .map_err(|e| format!("cannot sign registration: {e}"))?;
    if let Ok(bajty) = serde_json::to_vec(&rejestracja) {
        match swarm.behaviour_mut().gossipsub.publish(
            libp2p::gossipsub::IdentTopic::new(
                simon_harness::client_protocol::TOPIC_NODE_REGISTER,
            ),
            bajty,
        ) {
            Ok(_) => println!("[node] registered: model={model_deklarowany} max_ctx={max_ctx}"),
            Err(e) => eprintln!("[node] registration failed: {e}"),
        }
    }

    // Tasks are computed outside the swarm loop.
    let sloty = Arc::new(Semaphore::new(opcje.max_jobs));
    // Fixed queue limit per slot; requests wait within protocol REQUEST_TIMEOUT (300s).
    let przyjecia = Arc::new(Semaphore::new(opcje.max_jobs * KOLEJKA_NA_SLOT));
    let (gotowe_tx, mut gotowe_rx) = tokio::sync::mpsc::unbounded_channel();
    // Rate limiting and duplicate job rejection per peer to prevent queue exhaustion.
    let mut kubelki: std::collections::HashMap<libp2p::PeerId, simon_core::protocol::TokenBucket> =
        std::collections::HashMap::new();
    let mut ostatnie_joby: std::collections::VecDeque<(libp2p::PeerId, String)> =
        std::collections::VecDeque::new();
    let backend = Arc::new(Backend {
        model_url,
        model: model_deklarowany,
        runtime,
        klucz,
        max_ctx,
    });

    println!(
        "[node] waiting for prompts (point-to-point, /simon/prompt/1), max_jobs={} queue={}...",
        opcje.max_jobs,
        opcje.max_jobs * KOLEJKA_NA_SLOT
    );

    loop {
        let zdarzenie = tokio::select! {
            Some((channel, reply)) = gotowe_rx.recv() => {
                if let Err(e) = swarm.behaviour_mut().prompt_rr.send_response(channel, reply) {
                    eprintln!("[node] cannot return response (client disconnected?): {e:?}");
                }
                continue;
            }
            z = swarm.select_next_some() => z,
        };
        match zdarzenie {
            SwarmEvent::Behaviour(SwarmBehaviourEvent::PromptRr(RrEvent::Message {
                peer,
                message,
                ..
            })) => match message {
                RrMessage::Request {
                    request, channel, ..
                } => {
                    println!(
                        "[node] prompt from {peer}: order={} job={} ({} chars, max_ctx={})",
                        request.order_id,
                        request.job_id,
                        request.prompt.len(),
                        request.max_ctx
                    );
                    if let Some(reply) = sprawdz_wersje(&request) {
                        let _ = swarm.behaviour_mut().prompt_rr.send_response(channel, reply);
                        continue;
                    }
                    if let Some(reply) = check_peer_limits(
                        &mut kubelki,
                        &mut ostatnie_joby,
                        peer,
                        &request,
                        opcje.rate_per_peer,
                        teraz_us(),
                    ) {
                        let _ = swarm.behaviour_mut().prompt_rr.send_response(channel, reply);
                        continue;
                    }
                    // Admission permit held until task completes to prevent leak on panic.
                    let Ok(przyjete) = przyjecia.clone().try_acquire_owned() else {
                        let reply = blad(
                            &request,
                            "node_zajety",
                            &format!(
                                "node busy: {} jobs active or queued - retry later",
                                opcje.max_jobs * KOLEJKA_NA_SLOT
                            ),
                        );
                        let _ = swarm.behaviour_mut().prompt_rr.send_response(channel, reply);
                        continue;
                    };
                    let (b, sloty, tx) = (backend.clone(), sloty.clone(), gotowe_tx.clone());
                    tokio::spawn(async move {
                        let _przyjete = przyjete;
                        let _slot = sloty.acquire_owned().await.expect("semafor nigdy nie jest zamykany");
                        let reply = handle_request(&b, &request).await;
                        let _ = tx.send((channel, reply));
                    });
                }
                RrMessage::Response { .. } => {}
            },
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("[node] listening on: {address}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("[node] connected: {peer_id}");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                println!("[node] disconnected: {peer_id}");
            }
            _ => {}
        }
    }
}

/// Queue capacity per slot before returning node_busy.
const KOLEJKA_NA_SLOT: usize = 4;

const WERSJA_PROMPTU: u16 = 1;

/// Shared backend state across tasks.
pub struct Backend {
    pub model_url: String,
    pub model: String,
    pub runtime: String,
    pub klucz: Keypair,
    pub max_ctx: u32,
}

fn sprawdz_wersje(request: &PromptRequest) -> Option<PromptReply> {
    (request.v > WERSJA_PROMPTU).then(|| {
        blad(
            request,
            "wersja",
            &format!(
                "unsupported format version {} (node supports up to {})",
                request.v, WERSJA_PROMPTU
            ),
        )
    })
}

/// Single request handler: context gate, then compute completion. Runs in a separate
/// tokio task, never on the swarm loop.
pub async fn handle_request(b: &Backend, request: &PromptRequest) -> PromptReply {
    if let Some(reply) = sprawdz_wersje(request) {
        return reply;
    }
    let realny_ctx = odczytaj_realny_ctx(&b.model_url, &request.prompt).await;
    let ctx_do_bramki = match realny_ctx {
        Some(realny) => {
            if realny != request.max_ctx {
                println!(
                    "[node] /tokenize: {realny} tok. real vs {} declared (diff {:+.0}%)",
                    request.max_ctx,
                    (realny as f64 - request.max_ctx as f64)
                        / request.max_ctx.max(1) as f64
                        * 100.0
                );
            }
            realny
        }
        None => {
            eprintln!(
                "[node] WARNING: /tokenize unavailable, using agent declaration ({} tok.)",
                request.max_ctx
            );
            request.max_ctx
        }
    };
    if b.max_ctx > 0 && ctx_do_bramki > b.max_ctx {
        eprintln!(
            "[node] REJECTED: request requires {} context tokens, node limit is {}",
            ctx_do_bramki, b.max_ctx
        );
        blad_ctx(
            request,
            "ctx_ponad_limit",
            &format!(
                "request requires {} context tokens, node limit is {} (model {})",
                ctx_do_bramki, b.max_ctx, b.model
            ),
            Some(ctx_do_bramki),
            Some(b.max_ctx),
        )
    } else {
        compute_completion(&b.model_url, &b.model, &b.runtime, &b.klucz, request).await
    }
}

/// Computes prompt via OpenAI-compatible API (`/v1/chat/completions`).
pub async fn compute_completion(
    model_url: &str,
    model_hash: &str,
    runtime: &str,
    klucz: &Keypair,
    req: &PromptRequest,
) -> PromptReply {
    let start = Instant::now();
    let url = format!("{}/v1/chat/completions", model_url.trim_end_matches('/'));
    let body = match cialo_zapytania(model_hash, req) {
        Ok(b) => b,
        Err(e) => return blad(req, "zle_zlecenie", &e),
    };

    let klient = match klient_backendu()
        .timeout(Duration::from_secs(300))
        .build()
    {
        Ok(k) => k,
        Err(e) => return blad(req, "client", &format!("cannot build HTTP client: {e}")),
    };

    let started_at_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    let odp = match klient.post(&url).json(&body).send().await {
        Ok(o) => o,
        Err(e) => {
            return blad(
                req,
                "model_niedostepny",
                &format!("{url}: {e} (is backend running?)"),
            )
        }
    };

    let ttft_ms = start.elapsed().as_millis() as u64;
    let status = odp.status();
    let tekst = match odp.text().await {
        Ok(t) => t,
        Err(e) => return blad(req, "odczyt", &format!("cannot read response: {e}")),
    };

    let finished_at_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
        .max(started_at_us);

    if !status.is_success() {
        return blad(
            req,
            "model_blad",
            &format!("HTTP {status}: {}", tekst.chars().take(300).collect::<String>()),
        );
    }

    let v: serde_json::Value = match serde_json::from_str(&tekst) {
        Ok(v) => v,
        Err(e) => return blad(req, "json", &format!("invalid model response: {e}")),
    };

    let (output, tresc_pusta) = extract_output(&v, req.chat);
    if tresc_pusta {
        // Empty content returns as an error rather than signing internal reasoning.
        let finish = v["choices"][0]["finish_reason"].as_str().unwrap_or("nieznany");
        return blad(req, "pusta_tresc", &format!(
            "model returned empty content (finish_reason={finish}); tokens may have been spent on reasoning"));
    }

    let tokens_out = v["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;
    // Prefill bierzemy od backendu, a nie z /tokenize: `usage` jest dostepne
    // w obu silnikach, a /tokenize ma tylko vLLM.
    let prompt_tokens = v["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
    // NAPRAWA 2026-09-18: tu bylo `gen_ms = tokens_out / 65.0 * 1000` — stala
    // zmierzona kiedys dla Bielika 11B na 3080 Ti, stosowana do KAZDEGO wezla
    // i modelu. Komentarz obok twierdzil, ze to "twarda miara, nie heurystyka
    // tok/s", czyli dokladnie odwrotnie niz robil kod. Skutek: przepustowosc
    // liczona z tego pola wychodzila ZAWSZE 65,0 tok/s, niezaleznie od sprzetu.
    //
    // Teraz `gen_ms` to REALNIE ZMIERZONY czas wywolania backendu. Bez
    // streamingu nie da sie oddzielic prefilla od dekodowania, wiec ta liczba
    // obejmuje oba — i tak jest uczciwiej niz ekstrapolacja z cudzej stalej.
    // Prawdziwy TTFT bedzie dopiero przy streamingu.
    //
    // UWAGA: to nadal DEKLARACJA node'a (`node_declared_gen_ms`). Node mierzy
    // sam siebie, wiec moze sklamac. Nie wolno tego uzywac do rozliczen.
    let gen_ms = ttft_ms;

    println!(
        "[node] completed: {} tokens ({} output characters), backend_time={gen_ms}ms (measured, includes prefill+decode)",
        tokens_out,
        output.len()
    );
    // Non-empty output is guaranteed here after empty content check above.

    // M1: token binding for prompt and output up to MAX_M1_TOKENS (text mode with non-empty nonce)
    let m1_dane = if !req.chat && !req.nonce.trim().is_empty() {
        let p_tok = crate::common::tokenize_text(model_url, &req.prompt).await;
        let o_tok = crate::common::tokenize_text(model_url, &output).await;
        match (p_tok, o_tok) {
            (Some(p_ids), Some(o_ids))
                if !p_ids.is_empty()
                    && p_ids.len() <= crate::common::MAX_M1_TOKENS
                    && !o_ids.is_empty()
                    && o_ids.len() <= crate::common::MAX_M1_TOKENS =>
            {
                match crate::common::default_tokenizer_digest(model_hash) {
                    Ok(tok_hash) => Some((tok_hash, p_ids, o_ids)),
                    Err(e) => {
                        eprintln!("[node] cannot compute default tokenizer digest for {model_hash}: {e}");
                        None
                    }
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let m1_arg = m1_dane
        .as_ref()
        .map(|(th, p, o)| (th.as_str(), p.as_slice(), o.as_slice()));

    // Receipt BEFORE moving `output` — we sign exactly the text sent back.
    let receipt = build_signed_receipt(
        klucz,
        model_hash,
        runtime,
        req,
        &output,
        prompt_tokens,
        tokens_out,
        started_at_us,
        finished_at_us,
        m1_arg,
    );

    let receipt_ma_m1 = serde_json::from_str::<simon_core::receipt::Receipt>(&receipt)
        .map(|r| r.bindings.is_some())
        .unwrap_or(false);

    let (tokenizer_hash, prompt_token_ids, output_token_ids) = if receipt_ma_m1 {
        if let Some((th, p, o)) = m1_dane {
            (Some(th), Some(p), Some(o))
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    PromptReply::Ok(PromptResponse {
        v: 1,
        order_id: req.order_id.clone(),
        job_id: req.job_id.clone(),
        output,
        // M2.5.1: Signed receipt (Ed25519). Client verifies signature,
        // signer identity, job_id/model consistency, and content binding.
        receipt,
        tokens_out,
        ttft_ms,
        gen_ms,
        tokenizer_hash,
        prompt_token_ids,
        output_token_ids,
    })
}

/// Allowlisted chat request fields forwarded to backend.
const POLA_CZATU: &[&str] = &["tools", "tool_choice", "parallel_tool_calls", "temperature", "top_p", "stop"];

/// Request body for /v1/chat/completions. Supports text mode or structured chat mode.
fn cialo_zapytania(model: &str, req: &PromptRequest) -> Result<serde_json::Value, String> {
    if !req.chat {
        return Ok(serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": req.prompt}],
            "max_tokens": req.max_tokens,
            "temperature": 0.0,
        }));
    }
    let v: serde_json::Value = serde_json::from_str(&req.prompt)
        .map_err(|e| format!("tryb czatu: prompt nie jest JSON-em: {e}"))?;
    let wiadomosci = v
        .get("messages")
        .filter(|m| m.as_array().is_some_and(|a| !a.is_empty()))
        .ok_or("tryb czatu: brak niepustej tablicy `messages`")?;
    let mut body = serde_json::json!({
        "model": model,
        "messages": wiadomosci,
        "max_tokens": req.max_tokens,
        "temperature": 0.0,
    });
    for k in POLA_CZATU {
        if let Some(x) = v.get(*k) {
            body[*k] = x.clone();
        }
    }
    Ok(body)
}

/// Signed output content and empty flag check.
///
/// Extracts completion output. Empty content without tool calls returns empty flag.
pub fn extract_output(v: &serde_json::Value, chat: bool) -> (String, bool) {
    let msg = &v["choices"][0]["message"];
    let surowy = msg["content"].as_str().unwrap_or("");
    let narzedzia = msg
        .get("tool_calls")
        .filter(|t| t.as_array().is_some_and(|a| !a.is_empty()));
    if !chat {
        let tekst = surowy.trim().to_string();
        return (tekst.clone(), tekst.is_empty());
    }
    // Chat mode: raw model bytes without trim, check emptiness after trim.
    let tekst = if surowy.trim().is_empty() { String::new() } else { surowy.to_string() };
    let mut m = serde_json::Map::new();
    m.insert(
        "content".into(),
        if tekst.is_empty() && narzedzia.is_some() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(tekst.clone())
        },
    );
    // Empty content and no tool calls means empty reply.
    let pusta = tekst.is_empty() && narzedzia.is_none();
    if let Some(t) = narzedzia {
        m.insert("tool_calls".into(), t.clone());
    }
    m.insert("finish_reason".into(), v["choices"][0]["finish_reason"].clone());
    (serde_json::Value::Object(m).to_string(), pusta)
}

/// Probes backend for two facts: model id and max_model_len.
pub async fn probe_backend_facts(model_url: &str, zadany: Option<&str>) -> (Option<String>, u32) {
    let url = format!("{}/v1/models", model_url.trim_end_matches('/'));
    let klient = match klient_backendu()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(k) => k,
        Err(_) => return (None, 0),
    };
    let odp = match klient.get(&url).send().await {
        Ok(o) => o,
        Err(_) => return (None, 0),
    };
    let v: serde_json::Value = match odp.json().await {
        Ok(v) => v,
        Err(_) => return (None, 0),
    };
    let puste = Vec::new();
    let lista = v["data"].as_array().unwrap_or(&puste);
    let (id, max_ctx) = wybierz_wpis(lista, zadany);
    if max_ctx > 0 {
        return (id, max_ctx);
    }
    // llama.cpp does not report max_model_len in /v1/models; slot context is in /props.
    let props = format!("{}/props", model_url.trim_end_matches('/'));
    let n_ctx = match klient.get(&props).send().await {
        Ok(o) => o
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|p| p["default_generation_settings"]["n_ctx"].as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0),
        Err(_) => 0,
    };
    (id, n_ctx)
}

/// Klient HTTP do backendu modelu. `SIMON_MODEL_API_KEY` (np. AMD Token Factory)
// Read bearer token from environment variable rather than CLI arguments.
fn klient_backendu() -> reqwest::ClientBuilder {
    let b = reqwest::Client::builder();
    match std::env::var("SIMON_MODEL_API_KEY") {
        Ok(k) if !k.is_empty() => {
            let mut h = reqwest::header::HeaderMap::new();
            if let Ok(mut v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {k}")) {
                v.set_sensitive(true);
                h.insert(reqwest::header::AUTHORIZATION, v);
            }
            b.default_headers(h)
        }
        _ => b,
    }
}

/// Identifies matching model entry from /v1/models.
fn wybierz_wpis(lista: &[serde_json::Value], zadany: Option<&str>) -> (Option<String>, u32) {
    let wpis = match zadany {
        // Match exact model being served.
        Some(cel) => lista.iter().find(|m| m["id"].as_str() == Some(cel)),
        // Without `--model-hash`, data[0] is unambiguous only for a single model.
        None => match lista.len() {
            1 => lista.first(),
            _ => {
                if lista.len() > 1 {
                    eprintln!(
                        "[node] UWAGA: backend serwuje {} modeli, a nie podano --model-hash \
                         - ambiguous model configuration",
                        lista.len()
                    );
                }
                None
            }
        },
    };

    match wpis {
        Some(m) => (
            m["id"].as_str().map(|s| s.to_string()),
            m["max_model_len"].as_u64().unwrap_or(0) as u32,
        ),
        None => {
            if let Some(cel) = zadany {
                eprintln!(
                    "[node] UWAGA: backend NIE wystawia modelu '{cel}' (widzi {} innych)",
                    lista.len()
                );
            }
            (None, 0)
        }
    }
}

/// Detects backend engine runtime. Defaults to unknown/1 if undetermined.
pub async fn detect_runtime(model_url: &str) -> String {
    let baza = model_url.trim_end_matches('/');
    let klient = match klient_backendu()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(k) => k,
        Err(_) => return "nieznany/1".to_string(),
    };
    // Ollama: GET /api/version -> {"version":"0.x.y"}
    if let Ok(o) = klient.get(format!("{baza}/api/version")).send().await {
        if o.status().is_success() {
            if let Ok(v) = o.json::<serde_json::Value>().await {
                if let Some(w) = v["version"].as_str() {
                    return format!("ollama/{w}");
                }
            }
        }
    }
    // vLLM: GET /version -> {"version":"0.x.y"}
    if let Ok(o) = klient.get(format!("{baza}/version")).send().await {
        if o.status().is_success() {
            if let Ok(v) = o.json::<serde_json::Value>().await {
                if let Some(w) = v["version"].as_str() {
                    return format!("vllm/{w}");
                }
            }
        }
    }
    // llama.cpp: GET /props -> {"default_generation_settings": ..., "total_slots": ...}
    if let Ok(o) = klient.get(format!("{baza}/props")).send().await {
        if o.status().is_success() {
            if let Ok(v) = o.json::<serde_json::Value>().await {
                if v.get("default_generation_settings").is_some() || v.get("total_slots").is_some() {
                    return "llama.cpp/1".to_string();
                }
            }
        }
    }
    eprintln!("[node] WARNING: unrecognized engine at {baza} - receipt runtime set to 'unknown'");
    "nieznany/1".to_string()
}

/// Helper extracting token count from /tokenize endpoint.
pub(crate) fn wyciagnij_liczbe_tokenow(v: &serde_json::Value) -> Option<u64> {
    v.get("count")
        .and_then(|c| c.as_u64())
        .or_else(|| v.get("tokens").and_then(|t| t.as_array()).map(|a| a.len() as u64))
}

/// Queries prompt token count via /tokenize endpoint. Falls back to agent declaration on error.
async fn odczytaj_realny_ctx(model_url: &str, prompt: &str) -> Option<u32> {
    let url = format!("{}/tokenize", model_url.trim_end_matches('/'));
    let klient = match klient_backendu()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(k) => k,
        Err(e) => {
            eprintln!("[node] /tokenize: cannot build HTTP client: {e}");
            return None;
        }
    };
    let odp = match klient
        .post(&url)
        .json(&serde_json::json!({"prompt": prompt, "content": prompt}))
        .send()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[node] /tokenize: {url} unreachable: {e}");
            return None;
        }
    };
    let status = odp.status();
    if !status.is_success() {
        let tekst = odp.text().await.unwrap_or_default();
        eprintln!(
            "[node] /tokenize: HTTP {status}: {}",
            tekst.chars().take(200).collect::<String>()
        );
        return None;
    }
    let v: serde_json::Value = match odp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[node] /tokenize: response is not valid JSON: {e}");
            return None;
        }
    };
    match wyciagnij_liczbe_tokenow(&v) {
        // Range check: n > u32::MAX is rejected to prevent truncation.
        Some(n) => match u32::try_from(n) {
            Ok(n) => Some(n),
            Err(_) => {
                eprintln!(
                    "[node] /tokenize: token count {n} exceeds u32::MAX - falling back to agent declaration"
                );
                None
            }
        },
        None => {
            eprintln!("[node] /tokenize: response without `count` or `tokens` field: {v}");
            None
        }
    }
}

fn teraz_us() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0)
}

/// Cache of recent (peer, job_id) pairs to reject replays.
const PAMIEC_JOBOW: usize = 4096;

/// Per-peer rate limiting and replay rejection.
pub fn check_peer_limits(
    kubelki: &mut std::collections::HashMap<libp2p::PeerId, simon_core::protocol::TokenBucket>,
    ostatnie: &mut std::collections::VecDeque<(libp2p::PeerId, String)>,
    peer: libp2p::PeerId,
    req: &PromptRequest,
    rate: f64,
    teraz: u64,
) -> Option<PromptReply> {
    if ostatnie.iter().any(|(p, j)| *p == peer && *j == req.job_id) {
        return Some(blad(req, "powtorzony_job", "duplicate job_id from peer - replay is not computed twice"));
    }
    if rate > 0.0 {
        if kubelki.len() > 10_000 {
            kubelki.clear();
        }
        let k = kubelki
            .entry(peer)
            .or_insert_with(|| simon_core::protocol::TokenBucket::new(rate * 4.0, rate, teraz));
        if k.allow(teraz).is_err() {
            return Some(blad(req, "rate_limit", &format!("rate limit {rate} requests/s per peer exceeded - slow down")));
        }
    }
    ostatnie.push_back((peer, req.job_id.clone()));
    if ostatnie.len() > PAMIEC_JOBOW {
        ostatnie.pop_front();
    }
    None
}

/// Detects precision from model name tokens conservatively.
pub(crate) fn detect_precision_from_model_name(model_hash: &str) -> Precision {
    let n = model_hash.to_ascii_lowercase();
    let tokeny: Vec<&str> = n.split(|c: char| !c.is_ascii_alphanumeric()).filter(|t| !t.is_empty()).collect();
    // GGUF token parsing for quantization bits.
    let bity_gguf = |t: &str| -> Option<u32> {
        let r = t.strip_prefix('i').unwrap_or(t).strip_prefix('q')?;
        let mut z = r.chars();
        let b = z.next()?.to_digit(10)?;
        matches!(z.as_str(), "" | "k" | "ks" | "km" | "kl").then_some(b)
    };
    let mut jawne: Vec<Precision> = Vec::new();
    let mut nieobslugiwana = false;
    let mut slaby_int4 = false;
    for t in &tokeny {
        let p = if matches!(*t, "fp4" | "nvfp4" | "mxfp4") {
            Some(Precision::Fp4)
        } else if t.starts_with("fp8") {
            Some(Precision::Fp8)
        } else if matches!(*t, "int8" | "8bit") || t.starts_with("w8a") {
            Some(Precision::Int8)
        } else if matches!(*t, "int4" | "4bit") || t.starts_with("w4a") {
            Some(Precision::Int4)
        } else if let Some(b) = bity_gguf(t) {
            match b {
                8 => Some(Precision::Int8),
                4 => Some(Precision::Int4),
                _ => {
                    nieobslugiwana = true;
                    None
                }
            }
        } else {
            if matches!(*t, "awq" | "gptq") {
                slaby_int4 = true;
            }
            // general quantization tag without explicit width
            if matches!(*t, "quant" | "quantized") {
                nieobslugiwana = true;
            }
            None
        };
        if let Some(p) = p {
            if !jawne.contains(&p) {
                jawne.push(p);
            }
        }
    }
    // AWQ/GPTQ integer weights alongside fp8/fp4 ambiguity resolution.
    let sprzeczny_slaby = slaby_int4 && jawne.iter().any(|p| matches!(p, Precision::Fp8 | Precision::Fp4));
    if nieobslugiwana || jawne.len() > 1 || sprzeczny_slaby {
        return Precision::Unknown;
    }
    if let Some(p) = jawne.first() {
        return *p;
    }
    if slaby_int4 {
        return Precision::Int4;
    }
    let baza = |z: &[&str]| tokeny.iter().any(|t| z.contains(t));
    match (baza(&["bf16"]), baza(&["fp16", "f16"]), baza(&["fp32", "f32"])) {
        (true, false, false) => Precision::Bf16,
        (false, true, false) => Precision::Fp16,
        (false, false, true) => Precision::Fp32,
        _ => Precision::Unknown,
    }
}

/// M2.5.1: builds receipt and SIGNS it with node key.
#[allow(clippy::too_many_arguments)]
pub fn build_signed_receipt(
    klucz: &Keypair,
    model_hash: &str,
    runtime: &str,
    req: &PromptRequest,
    output: &str,
    prompt_tokens: u32,
    tokens_out: u32,
    started_at_us: u64,
    finished_at_us: u64,
    m1_dane: Option<(&str, &[u32], &[u32])>,
) -> String {
    let mut receipt = Receipt {
        job_id: req.job_id.clone(),
        node_id: klucz.public().to_hex(),
        model_hash: model_hash.to_string(),
        runtime: runtime.to_string(),
        precision: detect_precision_from_model_name(model_hash),
        // M2.5.1b: to be computed outside vLLM API (runner + hidden states).
        activation_hash: "toploc:NIE_POLICZONY".to_string(),
        // Odcisk TRESCI, nie metadanych. Metadane (czasy) i tak sa podpisane,
        // bo wchodza do odcisku calego receiptu.
        prompt_tokens,
        completion_tokens: tokens_out,
        output_digest: simon_core::receipt::output_digest_for(&req.job_id, output)
            .unwrap_or_else(|_| "digest-blad".to_string()),
        started_at_us,
        finished_at_us,
        signer: klucz.public(),
        signature: None,
        bindings: None,
    };

    if let Some((tok_hash, p_ids, o_ids)) = m1_dane {
        if let Ok(params_hash) = simon_core::receipt::SamplingCommitment::greedy().digest() {
            let exec_profile = format!("{runtime}/{:?}", receipt.precision).to_lowercase();
            match receipt.clone().with_m1_bindings(
                tok_hash.to_string(),
                req.nonce.clone(),
                p_ids,
                o_ids,
                params_hash,
                0,
                exec_profile,
            ) {
                Ok(r) => receipt = r,
                Err(e) => eprintln!("[node] failed to attach M1 bindings: {e}"),
            }
        }
    }

    match receipt.sign(klucz) {
        Ok(podpisany) => serde_json::to_string(&podpisany)
            .unwrap_or_else(|e| format!("{{\"blad\":\"serializacja: {e}\"}}")),
        Err(e) => {
            eprintln!("[node] receipt signature error: {e}");
            "{}".to_string()
        }
    }
}

fn blad(req: &PromptRequest, kod: &str, opis: &str) -> PromptReply {
    blad_ctx(req, kod, opis, None, None)
}

/// Constructs PromptReply::Error with structured context limit fields.
fn blad_ctx(
    req: &PromptRequest,
    kod: &str,
    opis: &str,
    realny_ctx: Option<u32>,
    limit_ctx: Option<u32>,
) -> PromptReply {
    eprintln!("[node] ERROR {kod}: {opis}");
    PromptReply::Error(PromptError {
        v: 1,
        order_id: req.order_id.clone(),
        code: kod.to_string(),
        description: opis.to_string(),
        actual_ctx: realny_ctx,
        limit_ctx,
    })
}


#[cfg(test)]
mod testy_wyboru_modelu {
    use super::wybierz_wpis;
    use serde_json::json;

    fn lista() -> Vec<serde_json::Value> {
        vec![
            json!({"id": "qwen2.5-coder:14b"}),
            json!({"id": "bielik:Q4", "max_model_len": 8192}),
            json!({"id": "gemma4:12b"}),
        ]
    }

    #[test]
    fn bierze_zadany_model_a_nie_pierwszy_z_brzegu() {
        // Regression 2026-09-18: ensure advertised model matches exactly.
        let (id, ctx) = wybierz_wpis(&lista(), Some("bielik:Q4"));
        assert_eq!(id.as_deref(), Some("bielik:Q4"));
        assert_eq!(ctx, 8192, "max_ctx must originate from matching model entry");
    }

    #[test]
    fn brak_zadanego_modelu_to_nie_jest_cichy_fallback() {
        let (id, ctx) = wybierz_wpis(&lista(), Some("model-ktorego-nie-ma"));
        assert!(id.is_none(), "must not advertise model absent from backend");
        assert_eq!(ctx, 0);
    }

    #[test]
    fn jeden_model_bez_wskazania_jest_jednoznaczny() {
        let jeden = vec![json!({"id": "qwen3.8-27b", "max_model_len": 36864})];
        let (id, ctx) = wybierz_wpis(&jeden, None);
        assert_eq!(id.as_deref(), Some("qwen3.8-27b"));
        assert_eq!(ctx, 36864);
    }

    #[test]
    fn kilka_modeli_bez_wskazania_to_zgadywanie_wiec_nic() {
        let (id, _) = wybierz_wpis(&lista(), None);
        assert!(id.is_none(), "with multiple models ambiguous selection is rejected");
    }
}

#[cfg(test)]
mod testy_wspolbieznosci {
    use crate::common::{p2p_key_from_seed, build_swarm};
    use simon_harness::request_response::{PromptReply, PromptRequest};
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const OPOZNIENIE: Duration = Duration::from_secs(2);

    /// Mock backend: simulated delay on completions, immediate response otherwise.
    async fn falszywy_backend() -> u16 {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = l.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 16384];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    if String::from_utf8_lossy(&buf[..n]).contains("chat/completions") {
                        tokio::time::sleep(OPOZNIENIE).await;
                    }
                    let body = r#"{"choices":[{"message":{"content":"ok"}}],"usage":{"completion_tokens":1,"prompt_tokens":1}}"#;
                    let odp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = s.write_all(odp.as_bytes()).await;
                });
            }
        });
        port
    }

    fn backend_i_zlecenie(port: u16, v: u16, chat: bool) -> (super::Backend, PromptRequest) {
        (
            super::Backend {
                model_url: format!("http://127.0.0.1:{port}"),
                model: "m".into(),
                runtime: "test".into(),
                klucz: simon_core::crypto::Keypair::from_seed(&[7; 32]),
                max_ctx: 1,
            },
            PromptRequest {
                v,
                order_id: "wersja-test".into(),
                job_id: "j".into(),
                model_hash: "m".into(),
                prompt: if chat {
                    r#"{"messages":[{"role":"user","content":"hej"}]}"#.into()
                } else {
                    "hej".into()
                },
                max_tokens: 1,
                max_ctx: 1,
                nonce: "n".into(),
                chat,
            },
        )
    }

    #[tokio::test]
    async fn nowsza_wersja_jest_odrzucana_przed_kontaktem_z_backendem() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        for chat in [false, true] {
            for v in [super::WERSJA_PROMPTU + 1, u16::MAX] {
                let (mut backend, mut req) = backend_i_zlecenie(port, v, chat);
                // Version error check takes priority.
                backend.max_ctx = 1;
                req.max_ctx = 2;
                req.prompt = "nie json".into();
                let reply = tokio::time::timeout(Duration::from_secs(1), super::handle_request(&backend, &req))
                    .await
                    .expect("odrzucenie wersji nie czeka na backend");
                let PromptReply::Error(err) = reply else {
                    panic!("expected version error");
                };
                assert_eq!(err.code, "wersja");
                assert_eq!(err.order_id, req.order_id);
                assert_eq!(err.v, 1);
                assert_eq!(err.actual_ctx, None);
                assert_eq!(err.limit_ctx, None);
                assert!(err.description.contains(&v.to_string()));
            }
        }
        assert!(tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn obslugiwana_i_starsza_wersja_dochodza_do_backendu() {
        let port = falszywy_backend().await;
        for chat in [false, true] {
            for v in [0, super::WERSJA_PROMPTU] {
                let (backend, req) = backend_i_zlecenie(port, v, chat);
                let reply = super::handle_request(&backend, &req).await;
                assert!(matches!(reply, PromptReply::Ok(_)), "{reply:?}");
            }
        }
    }

    /// Regression 2026-10-11: verify concurrent prompt handling up to max_jobs.
    #[tokio::test(flavor = "multi_thread")]
    async fn cztery_prompty_licza_sie_naraz() {
        let backend = falszywy_backend().await;
        let wolny = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = wolny.local_addr().unwrap().port();
        drop(wolny);
        let seed = [7u8; 32];
        let peer = p2p_key_from_seed(&seed).public().to_peer_id();
        let args: Vec<String> = [
            "--role", "node",
            "--listen", &format!("/ip4/127.0.0.1/tcp/{port}"),
            "--model-url", &format!("http://127.0.0.1:{backend}"),
            "--key", &hex::encode(seed),
            "--max-jobs", "4",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let opcje = crate::Opcje::parse(&args).unwrap();
        tokio::spawn(async move { super::run(&opcje).await });
        tokio::time::sleep(Duration::from_secs(1)).await;

        async fn wyslij_i_czekaj(
            swarm: &mut libp2p::Swarm<crate::common::SwarmBehaviour>,
            peer: libp2p::PeerId,
            request: PromptRequest,
        ) -> Result<(PromptReply, u64), String> {
            use futures::StreamExt;
            use libp2p::request_response::{Event as RrEvent, Message as RrMessage};
            use libp2p::swarm::SwarmEvent;
            let start = std::time::Instant::now();
            let req_id = swarm.behaviour_mut().prompt_rr.send_request(&peer, request);
            loop {
                match swarm.select_next_some().await {
                    SwarmEvent::Behaviour(crate::common::SwarmBehaviourEvent::PromptRr(
                        RrEvent::Message {
                            message: RrMessage::Response {
                                request_id,
                                response,
                            },
                            ..
                        },
                    )) if request_id == req_id => {
                        return Ok((response, start.elapsed().as_millis() as u64));
                    }
                    _ => {}
                }
            }
        }

        let adres = format!("/ip4/127.0.0.1/tcp/{port}/p2p/{peer}");
        let start = Instant::now();
        let agenci: Vec<_> = (0..4)
            .map(|i| {
                let adres = adres.clone();
                tokio::spawn(async move {
                    let mut swarm = build_swarm(&[], &[adres], None).unwrap();
                    let req = PromptRequest {
                        v: 1,
                        order_id: format!("o{i}"),
                        job_id: format!("j{i}"),
                        model_hash: "m".into(),
                        prompt: "hej".into(),
                        max_tokens: 1,
                        max_ctx: 1,
                        nonce: "n".into(),
                        chat: false,
                    };
                    wyslij_i_czekaj(&mut swarm, peer, req).await
                })
            })
            .collect();
        for a in agenci {
            let (reply, _) = a.await.unwrap().expect("agent received reply");
            assert!(matches!(reply, PromptReply::Ok(_)), "{reply:?}");
        }
        let czas = start.elapsed();
        assert!(
            czas < OPOZNIENIE * 3,
            "4 prompts took {czas:?} - should be computed concurrently"
        );
    }
}

#[cfg(test)]
mod testy_czatu {
    use super::{cialo_zapytania, extract_output};
    use serde_json::json;
    use simon_harness::request_response::PromptRequest;

    fn req(prompt: &str, chat: bool) -> PromptRequest {
        PromptRequest {
            v: 1,
            order_id: "o".into(),
            job_id: "j".into(),
            model_hash: "m".into(),
            prompt: prompt.into(),
            max_tokens: 50,
            max_ctx: 10,
            nonce: "n".into(),
            chat,
        }
    }

    #[test]
    fn czat_przekazuje_narzedzia_z_bialej_listy_i_nic_poza_nia() {
        let p = json!({"messages": [{"role": "user", "content": "hej"}],
                       "tools": [{"type": "function", "function": {"name": "ls"}}],
                       "temperature": 0.7, "stream": true, "n": 5}).to_string();
        let b = cialo_zapytania("m", &req(&p, true)).unwrap();
        assert_eq!(b["tools"][0]["function"]["name"], "ls");
        assert_eq!(b["temperature"], 0.7, "parametr klienta, nie wymuszone 0");
        assert!(b.get("stream").is_none() && b.get("n").is_none(), "field outside allowlist: {b}");
        assert!(cialo_zapytania("m", &req(r#"{"messages": []}"#, true)).is_err());
        assert!(cialo_zapytania("m", &req("nie json", true)).is_err());
        // text mode: prompt wrapped as single user message
        let t = cialo_zapytania("m", &req("nie json", false)).unwrap();
        assert_eq!(t["messages"][0]["content"], "nie json");
    }

    #[test]
    fn czat_podpisuje_wywolania_narzedzi_i_finish_reason() {
        let v = json!({"choices": [{"finish_reason": "tool_calls", "message": {"content": null,
            "tool_calls": [{"id": "c1", "type": "function",
                            "function": {"name": "read_file", "arguments": "{\"path\":\"a.rs\"}"}}]}}]});
        let (out, _) = extract_output(&v, true);
        let m: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(m["content"].is_null());
        assert_eq!(m["tool_calls"][0]["function"]["arguments"], "{\"path\":\"a.rs\"}");
        assert_eq!(m["finish_reason"], "tool_calls");
        // truncated call remains truncated with finish_reason preserved
        let v2 = json!({"choices": [{"finish_reason": "length", "message": {"content": "abc"}}]});
        assert_eq!(serde_json::from_str::<serde_json::Value>(&extract_output(&v2, true).0).unwrap()["finish_reason"], "length");
        // text mode: raw text output preserved
        assert_eq!(extract_output(&v2, false).0, "abc");
        let v3 = json!({"choices": [{"finish_reason": "length", "message": {"content": "", "reasoning": "reasoning"}}]});
        let (tekst3, pusta3) = extract_output(&v3, false);
        assert_eq!(tekst3, "", "reasoning must not become signed output");
        assert!(pusta3, "empty content must be flagged as empty");
        let (out3_chat, pusta3_chat) = extract_output(&v3, true);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&out3_chat).unwrap()["content"], "");
        assert!(pusta3_chat, "empty chat content without tool calls is empty");
    }

    #[test]
    fn czat_z_narzedziami_nie_jest_pusty_mimo_content_null() {
        let v = json!({"choices": [{"finish_reason": "tool_calls", "message": {"content": null,
            "tool_calls": [{"id": "c1", "type": "function",
                            "function": {"name": "ls", "arguments": "{}"}}]}}]});
        let (_, pusta) = extract_output(&v, true);
        assert!(!pusta, "tool calls without content is not an empty reply");
    }
}

#[cfg(test)]
mod testy_receiptu {
    use super::*;
    use simon_harness::request_response::PromptRequest;

    #[test]
    fn test_receipt_timestamps_prawdziwe_i_uporzadkowane() {
        let klucz = Keypair::generate();
        let req = PromptRequest {
            v: 1,
            order_id: "ord-1".into(),
            job_id: "job-1".into(),
            model_hash: "gemma-4".into(),
            prompt: "test".into(),
            max_tokens: 16,
            max_ctx: 128,
            nonce: "nonce-1".into(),
            chat: false,
        };
        let started = 1_700_000_000_000_000;
        let finished = 1_700_000_000_500_000;
        let json_receipt = build_signed_receipt(
            &klucz, "gemma-4", "llama.cpp/1", &req, "odpowiedz", 10, 5, started, finished, None,
        );
        let r: Receipt = serde_json::from_str(&json_receipt).unwrap();
        assert_eq!(r.started_at_us, started);
        assert_eq!(r.finished_at_us, finished);
        assert!(r.started_at_us <= r.finished_at_us);
        assert!(r.verify_self().is_ok());
    }

    #[test]
    fn test_build_signed_receipt_m1_and_fallback_m0() {
        let klucz = Keypair::generate();
        let req = PromptRequest {
            v: 1,
            order_id: "ord-m1".into(),
            job_id: "job-m1".into(),
            model_hash: "qwen-fp16".into(),
            prompt: "witaj".into(),
            max_tokens: 16,
            max_ctx: 128,
            nonce: "nonce-xyz".into(),
            chat: false,
        };
        let started = 1_700_000_000_000_000;
        let finished = 1_700_000_000_500_000;
        let p_ids = [101, 202, 303];
        let o_ids = [404, 505];
        let tok_hash = "tokenizer-sha256";

        // 1. Level 1: when M1 data is provided, a valid level 1 receipt is built
        let json_m1 = build_signed_receipt(
            &klucz,
            "qwen-fp16",
            "vllm/0.6.3",
            &req,
            "odpowiedz",
            3,
            2,
            started,
            finished,
            Some((tok_hash, &p_ids, &o_ids)),
        );
        let r1: Receipt = serde_json::from_str(&json_m1).expect("parsowanie receiptu M1");
        assert_eq!(r1.level(), 1);
        assert!(r1.bindings.is_some());
        assert!(r1.verify_self().is_ok());
        assert!(r1.matches_output("odpowiedz"));
        assert!(r1.verify_m1(tok_hash, &req.nonce, &p_ids, &o_ids).is_ok());

        // 2. Level 0 fallback: without M1 data (None), receipt has level 0
        let json_m0 = build_signed_receipt(
            &klucz,
            "qwen-fp16",
            "vllm/0.6.3",
            &req,
            "odpowiedz",
            3,
            2,
            started,
            finished,
            None,
        );
        let r0: Receipt = serde_json::from_str(&json_m0).expect("parsowanie receiptu M0");
        assert_eq!(r0.level(), 0);
        assert!(r0.bindings.is_none());
        assert!(r0.verify_self().is_ok());
        assert!(r0.matches_output("odpowiedz"));
    }

    #[test]
    fn check_peer_limits_rejects_replay_and_excess() {
        use std::collections::{HashMap, VecDeque};
        let (mut k, mut o) = (HashMap::new(), VecDeque::new());
        let peer = libp2p::PeerId::random();
        let inny = libp2p::PeerId::random();
        let req = |job: &str| PromptRequest {
            v: 1, order_id: "o".into(), job_id: job.into(), model_hash: "m".into(), prompt: "p".into(),
            max_tokens: 1, max_ctx: 8, nonce: "n".into(), chat: false,
        };
        let kod = |r: Option<PromptReply>| match r { Some(PromptReply::Error(e)) => Some(e.code), _ => None };
        assert_eq!(kod(check_peer_limits(&mut k, &mut o, peer, &req("j1"), 1.0, 0)), None);
        assert_eq!(kod(check_peer_limits(&mut k, &mut o, peer, &req("j1"), 1.0, 0)).as_deref(), Some("powtorzony_job"));
        for j in ["j2", "j3", "j4"] {
            assert_eq!(kod(check_peer_limits(&mut k, &mut o, peer, &req(j), 1.0, 0)), None, "{j}");
        }
        assert_eq!(kod(check_peer_limits(&mut k, &mut o, peer, &req("j5"), 1.0, 0)).as_deref(), Some("rate_limit"));
        assert_eq!(kod(check_peer_limits(&mut k, &mut o, inny, &req("j5"), 1.0, 0)), None);
        assert_eq!(kod(check_peer_limits(&mut k, &mut o, peer, &req("j6"), 1.0, 2_000_000)), None);
        let (mut k2, mut o2) = (HashMap::new(), VecDeque::new());
        for i in 0..100 {
            assert_eq!(kod(check_peer_limits(&mut k2, &mut o2, peer, &req(&format!("x{i}")), 0.0, 0)), None);
        }
    }

    #[test]
    fn test_detect_precision_from_model_name() {
        assert_eq!(detect_precision_from_model_name("gemma-4-26b-a4b-it-bf16"), Precision::Bf16);
        assert_eq!(detect_precision_from_model_name("gemma-4-26b-a4b-it-qat-q4_0"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("qwen-q8_0"), Precision::Int8);
        assert_eq!(detect_precision_from_model_name("model-awq"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("model-gptq"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("model-int4"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("model-fp8"), Precision::Fp8);
        assert_eq!(detect_precision_from_model_name("model-nvfp4"), Precision::Fp4);
        assert_eq!(detect_precision_from_model_name("model-fp32"), Precision::Fp32);
        assert_eq!(detect_precision_from_model_name("model-f32"), Precision::Fp32);
        assert_eq!(detect_precision_from_model_name("model-fp16"), Precision::Fp16);
        assert_eq!(detect_precision_from_model_name("standard-model"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("deepseek-v4-flash-amd"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("llama-q5_k_m"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("gemma-4-26b-bf16-q4_0"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("llama-fp16-awq"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("model-gptq-8bit"), Precision::Int8);
        assert_eq!(detect_precision_from_model_name("model-bf16-iq4_xs"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("llama-q4km"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("qwen-q8_k"), Precision::Int8);
        assert_eq!(detect_precision_from_model_name("model-w4a16"), Precision::Int4);
        assert_eq!(detect_precision_from_model_name("model-w8a8"), Precision::Int8);
        assert_eq!(detect_precision_from_model_name("model-fp8e4m3"), Precision::Fp8);
        assert_eq!(detect_precision_from_model_name("model-bf16-q6_k"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("llama-int4-fp8"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("llama-awq-fp8"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("llama-gptq-fp4"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("model-bf16-quantized"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("model-q80"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("bf16-q8deadbeef"), Precision::Bf16);
        assert_eq!(detect_precision_from_model_name("merge-bf16-fp16"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("model@0a8f16c3bf16e9f32d"), Precision::Unknown);
        assert_eq!(detect_precision_from_model_name("qwen2.5-32b-instruct"), Precision::Unknown);
        assert_eq!(serde_json::to_string(&Precision::Int4).unwrap(), "\"int4\"");
        assert_eq!(serde_json::to_string(&Precision::Unknown).unwrap(), "\"unknown\"");
    }

    #[test]
    fn test_tokenize_llama_cpp_format() {
        let v_vllm = serde_json::json!({"count": 42});
        assert_eq!(wyciagnij_liczbe_tokenow(&v_vllm), Some(42));

        let v_llama = serde_json::json!({"tokens": [10, 20, 30]});
        assert_eq!(wyciagnij_liczbe_tokenow(&v_llama), Some(3));

        let v_inny = serde_json::json!({"other": "field"});
        assert_eq!(wyciagnij_liczbe_tokenow(&v_inny), None);
    }
}
