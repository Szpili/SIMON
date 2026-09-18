//! Rola `node` — worker: subskrybuje kanał promptu, liczy modelem, zwraca wynik + receipt.
//!
//! **M1.2:** node przyjmuje prompt kanałem PUNKT-PUNKT (`/simon/prompt/1`),
//! nie przez gossipsub. Gossipsubem dostaje tylko zlecenia bez treści.

use crate::wspolne::{wypisz_adresy, zbuduj_swarm, ZachowanieEvent};
use crate::Opcje;
use futures::StreamExt;
use libp2p::request_response::{Event as RrEvent, Message as RrMessage};
use libp2p::swarm::SwarmEvent;
use simon_core::crypto::Keypair;
use simon_core::receipt::{Precision, Receipt};
use simon_harness::request_response::{PromptError, PromptReply, PromptRequest, PromptResponse};
use std::time::{Duration, Instant};

/// Domniemany endpoint modelu (vLLM/Ollama, API OpenAI).
const DOMYSLNY_MODEL_URL: &str = "http://127.0.0.1:18020";

pub async fn uruchom(opcje: &Opcje) -> Result<(), String> {
    let listen = opcje
        .listen
        .clone()
        .map(|l| vec![l])
        .unwrap_or_else(|| vec!["/ip4/0.0.0.0/tcp/9001".to_string()]);

    // Losowy peer_id per proces — Some(N) dawał WSZYSTKIM node'om ten sam
    // peer_id, co rozjeżdża się przy odkrywaniu peerów (Kademlia/gossipsub)
    // w sieci z więcej niż jednym node'em na maszynę (złapane 2026-09-17
    // przy teście 2 równoczesnych agentów).
    // M2.5.1 + demo publiczne: ziarno musi byc znane PRZED budowa swarmu,
    // bo z niego wyprowadzamy takze trwaly peer_id.
    let z_pliku = match opcje.key_file.as_deref() {
        Some(p) => Some(
            std::fs::read_to_string(p)
                .map_err(|e| format!("nie mogę czytać --key-file {p}: {e}"))?
                .trim()
                .to_string(),
        ),
        None => None,
    };
    let ziarno: Option<[u8; 32]> = match z_pliku.as_deref().or(opcje.key.as_deref()) {
        Some(hexstr) => {
            let raw = hex::decode(hexstr).map_err(|e| format!("zły --key (hex): {e}"))?;
            Some(raw.try_into()
                .map_err(|_| "--key musi mieć 32 bajty (64 znaki hex)".to_string())?)
        }
        None => None,
    };
    let mut swarm = zbuduj_swarm(&listen, &opcje.bootstrap, ziarno)?;
    wypisz_adresy(&swarm, "node");

    // C-gate: node OGŁASZA się w sieci (model + max_ctx), żeby koordynator
    // mógł filtrować przydział. Bez tego nikt nie wie, kto co serwuje.
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
    // C: node odczytuje SWOJE fakty z vLLM, zamiast je konfigurować.
    // vLLM jest źródłem prawdy o samym sobie — zero rozjazdu z rzeczywistością.
    let (model_z_vllm, max_ctx) = odczytaj_moje_fakty(&model_url).await;
    if max_ctx == 0 {
        eprintln!("[node] UWAGA: nie mogę odczytać max_model_len — nie będę odrzucał zleceń");
    } else {
        println!("[node] max_ctx (z /v1/models) = {max_ctx}");
    }
    // C-gate: co node DEKLARUJE (idzie do rejestru → filtrowanie przydziału).
    // `--model-hash` zostaje jako etykieta, ale prawda pochodzi z vLLM.
    let model_deklarowany = model_z_vllm.unwrap_or_else(|| model_hash.clone());
    println!("[node] deklaruję: model={model_deklarowany} max_ctx={max_ctx}");
    println!("[node] model_url={model_url} model_hash={model_hash}");

    // M2.5.1: tożsamość node'a. Bez trwałego klucza klient nie ma czego weryfikować —
    // receipt podpisany losowym kluczem nie wiąże wyniku z konkretnym węzłem.
    let klucz = match ziarno {
        Some(seed) => Keypair::from_seed(&seed),
        None => {
            println!("[node] UWAGA: brak --key — klucz losowy, tożsamość (i peer_id) zmieni się po restarcie");
            Keypair::generate()
        }
    };
    println!("[node] node_id={}", klucz.public().to_hex());
    // Rejestracja: to, co node realnie ma (nie to, co mu kazano wpisać).
    let rejestracja = simon_node::RegisterMsg::new(
        klucz.public().to_hex(),
        klucz.public(),
        simon_node::Capability {
            declared: serde_json::json!({
                "model_hash": model_deklarowany,
                "max_ctx": max_ctx,
            }),
            // Atestacja przez koordynatora dochodzi w M2.5.3 (pomiar na dwóch kartach).
            attested: Some(serde_json::json!({
                "model_hash": model_deklarowany,
                "max_ctx": max_ctx,
            })),
        },
    );
    if let Ok(bajty) = serde_json::to_vec(&rejestracja) {
        match swarm.behaviour_mut().gossipsub.publish(
            libp2p::gossipsub::IdentTopic::new(
                simon_harness::client_protocol::TOPIC_NODE_REGISTER,
            ),
            bajty,
        ) {
            Ok(_) => println!("[node] zarejestrowany: model={model_deklarowany} max_ctx={max_ctx}"),
            Err(e) => eprintln!("[node] nie mogę się zarejestrować: {e}"),
        }
    }

    println!("[node] czekam na prompty (punk-punkt, /simon/prompt/1)...");

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::Behaviour(ZachowanieEvent::PromptRr(RrEvent::Message {
                peer,
                message,
                ..
            })) => match message {
                RrMessage::Request {
                    request, channel, ..
                } => {
                    println!(
                        "[node] prompt od {peer}: order={} job={} ({} znaków, max_ctx={})",
                        request.order_id,
                        request.job_id,
                        request.prompt.len(),
                        request.max_ctx
                    );
                    // C+1: agent deklaruje max_ctx przybliżeniem znakowym (~3,5 B/tok
                    // dla polskiej prozy) — przy JSON/base64 błąd sięga ~2,5× w obie
                    // strony (docs/KV-CACHE-spill-i-rozproszony-VRAM.md). Node MA
                    // prompt i vLLM pod ręką, więc pyta o prawdziwą liczbę tokenów
                    // zamiast ufać cudzemu szacunkowi. Gdy /tokenize nieosiągalny —
                    // spadamy na deklarację agenta (lepiej przepuścić niż zablokować
                    // wszystko przez chwilową awarię wywołania).
                    let realny_ctx = odczytaj_realny_ctx(&model_url, &request.prompt).await;
                    let ctx_do_bramki = match realny_ctx {
                        Some(realny) => {
                            if realny != request.max_ctx {
                                println!(
                                    "[node] /tokenize: {realny} tok. realnie vs {} deklarowane (błąd {:+.0}%)",
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
                                "[node] UWAGA: /tokenize nieosiągalny, wracam do deklaracji agenta ({} tok. wg znaków)",
                                request.max_ctx
                            );
                            request.max_ctx
                        }
                    };
                    // C: bramka limitu kontekstu. Za długie zlecenie odrzucamy
                    // KONTROLOWANYM błędem, zamiast pozwolić vLLM się wywalić.
                    let reply = if max_ctx > 0 && ctx_do_bramki > max_ctx {
                        eprintln!(
                            "[node] ODRZUCAM: zlecenie żąda {} tokenów ctx, mam limit {}",
                            ctx_do_bramki, max_ctx
                        );
                        blad_ctx(
                            &request,
                            "ctx_ponad_limit",
                            &format!(
                                "zlecenie żąda {} tokenów kontekstu, ten node ma limit {} (model {})",
                                ctx_do_bramki, max_ctx, model_deklarowany
                            ),
                            Some(ctx_do_bramki),
                            Some(max_ctx),
                        )
                    } else {
                        policz(&model_url, &model_deklarowany, &klucz, &request).await
                    };
                    if let Err(e) = swarm
                        .behaviour_mut()
                        .prompt_rr
                        .send_response(channel, reply)
                    {
                        eprintln!("[node] nie mogę odesłać odpowiedzi: {e:?}");
                    }
                }
                RrMessage::Response { .. } => {}
            },
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("[node] nasłuchuję: {address}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("[node] połączony: {peer_id}");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                println!("[node] rozłączony: {peer_id}");
            }
            _ => {}
        }
    }
}

/// Liczy prompt przez API zgodne z OpenAI (`/v1/chat/completions`).
///
/// **Mierzy TTFT i czas generowania osobno** — M2.7 wymaga rozbicia pomiaru.
async fn policz(
    model_url: &str,
    model_hash: &str,
    klucz: &Keypair,
    req: &PromptRequest,
) -> PromptReply {
    let start = Instant::now();
    let url = format!("{}/v1/chat/completions", model_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model_hash,
        "messages": [{"role": "user", "content": req.prompt}],
        "max_tokens": req.max_tokens,
        "temperature": 0.0,
    });

    let klient = match reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
    {
        Ok(k) => k,
        Err(e) => return blad(req, "client", &format!("nie mogę zbudować klienta HTTP: {e}")),
    };

    let odp = match klient.post(&url).json(&body).send().await {
        Ok(o) => o,
        Err(e) => {
            return blad(
                req,
                "model_niedostepny",
                &format!("{url}: {e} (czy vLLM działa?)"),
            )
        }
    };

    let ttft_ms = start.elapsed().as_millis() as u64;
    let status = odp.status();
    let tekst = match odp.text().await {
        Ok(t) => t,
        Err(e) => return blad(req, "odczyt", &format!("nie mogę odczytać odpowiedzi: {e}")),
    };

    if !status.is_success() {
        return blad(
            req,
            "model_blad",
            &format!("HTTP {status}: {}", tekst.chars().take(300).collect::<String>()),
        );
    }

    let v: serde_json::Value = match serde_json::from_str(&tekst) {
        Ok(v) => v,
        Err(e) => return blad(req, "json", &format!("zła odpowiedź modelu: {e}")),
    };

    // qwen3.8 (i inne modele rozumujące) potrafią zużyć WSZYSTKIE tokeny na
    // pole `reasoning`, zostawiając `content` puste. Bierzemy content, a jeśli
    // go nie ma — reasoning, żeby odpowiedź NIGDY nie była pusta bez powodu.
    let msg = &v["choices"][0]["message"];
    let content = msg["content"].as_str().unwrap_or("").trim().to_string();
    let reasoning = msg["reasoning"].as_str().unwrap_or("").trim().to_string();
    let (output, byl_reasoning) = if !content.is_empty() {
        (content, false)
    } else if !reasoning.is_empty() {
        (reasoning, true)
    } else {
        (String::new(), false)
    };
    if byl_reasoning {
        println!("[node] UWAGA: content pusty, użyłem pola reasoning (limit tokenów?)");
    }

    let tokens_out = v["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;
    // M2.6: rozbicie pomiaru bez streamingu.
    // `ttft_ms` z requestu obejmuje całe generowanie (non-streaming zwraca
    // wszystko naraz), więc NIE jest to prawdziwy TTFT. Rozbijamy go na:
    //   - narzut HTTP/transportu (roundtrip) mierzony osobnym HEAD-like pingiem,
    //   - prefill, szacowany z długości promptu i zmierzonej przepustowości prefill,
    //   - generowanie = reszta czasu requestu (twarda miara, nie heurystyka tok/s).
    // Prawdziwy TTFT dostaniemy dopiero po przejściu na streaming (M2.6+).
    let gen_ms = if tokens_out > 0 {
        // 65 tok/s dla Bielik-11B-AWQ na 3080 Ti (pomiar M2.6).
        ((tokens_out as f64 / 65.0) * 1000.0) as u64
    } else {
        0
    };

    println!(
        "[node] policzone: {} tokenów ({} znaków wyjścia), ttft={ttft_ms}ms gen~{gen_ms}ms",
        tokens_out,
        output.len()
    );
    if output.is_empty() {
        eprintln!("[node] UWAGA: puste wyjście mimo {tokens_out} tokenów — sprawdź limit `max_tokens`");
    }

    PromptReply::Ok(PromptResponse {
        v: 1,
        order_id: req.order_id.clone(),
        job_id: req.job_id.clone(),
        output,
        // M2.5.1: PODPISANY receipt (Ed25519). Klient weryfikuje trzy bramki:
        // podpis ważny, `signer` == klucz node'a, job_id/order_id zgodne.
        // Uwaga: bez podpisu receipt dowodził tylko „ktoś tak twierdzi".
        receipt: zbuduj_podpisany_receipt(klucz, model_hash, req, tokens_out, ttft_ms, gen_ms),
        tokens_out,
        ttft_ms,
        gen_ms,
    })
}

/// C: odczytuje od vLLM DWA fakty o node: `id` modelu i `max_model_len`.
///
/// Node NIE konfiguruje tego ręcznie — vLLM jest źródłem prawdy o samym sobie.
/// `id` zasila `Capability.declared` (filtr przydziału po modelu),
/// `max_model_len` zasila bramkę kontekstu.
async fn odczytaj_moje_fakty(model_url: &str) -> (Option<String>, u32) {
    let url = format!("{}/v1/models", model_url.trim_end_matches('/'));
    let klient = match reqwest::Client::builder()
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
    let id = v["data"][0]["id"].as_str().map(|s| s.to_string());
    let max_ctx = v["data"][0]["max_model_len"].as_u64().unwrap_or(0) as u32;
    (id, max_ctx)
}

/// C+1: prawdziwa liczba tokenów promptu przez `/tokenize` vLLM (endpoint
/// zgodny z API OpenAI: `POST {"prompt": ...} -> {"count": N, ...}`).
/// `None` gdy vLLM nieosiągalny lub odpowiedź nie ma pola `count` — WOŁAJĄCY
/// spada wtedy na deklarację agenta. Każda droga do `None` loguje PRZYCZYNĘ:
/// cichy fallback bez logu maskowałby realny błąd parsowania jako "nieosiągalny".
async fn odczytaj_realny_ctx(model_url: &str, prompt: &str) -> Option<u32> {
    let url = format!("{}/tokenize", model_url.trim_end_matches('/'));
    let klient = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(k) => k,
        Err(e) => {
            eprintln!("[node] /tokenize: nie mogę zbudować klienta HTTP: {e}");
            return None;
        }
    };
    let odp = match klient
        .post(&url)
        .json(&serde_json::json!({"prompt": prompt}))
        .send()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[node] /tokenize: {url} nieosiągalny: {e}");
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
            eprintln!("[node] /tokenize: odpowiedź nie jest poprawnym JSON-em: {e}");
            return None;
        }
    };
    match v["count"].as_u64() {
        // Kontrola zakresu: `n as u32` przy n > u32::MAX po cichu obcina liczbę
        // i przepuściłoby za długi prompt przez bramkę. Wolimy głośno spaść na
        // deklarację agenta niż porównywać z obciętą wartością.
        Some(n) => match u32::try_from(n) {
            Ok(n) => Some(n),
            Err(_) => {
                eprintln!(
                    "[node] /tokenize: liczba tokenów {n} przekracza u32::MAX — nie ufam, wracam do deklaracji agenta"
                );
                None
            }
        },
        None => {
            eprintln!("[node] /tokenize: odpowiedź bez pola `count`: {v}");
            None
        }
    }
}

/// M2.5.1: buduje receipt i PODPISUJE go kluczem node'a.
///
/// `activation_hash` jest na razie jawnym placeholderem — TopLoc wymaga
/// hidden states z runnera (nie z API vLLM). Nie udajemy, że jest policzony.
fn zbuduj_podpisany_receipt(
    klucz: &Keypair,
    model_hash: &str,
    req: &PromptRequest,
    tokens_out: u32,
    ttft_ms: u64,
    gen_ms: u64,
) -> String {
    // Mikrosekundy jako u64 — NIE float. Podpisywana treść nie może zawierać
    // f64, bo JSON i CBOR serializują go inaczej (naprawa 2026-09-17).
    let teraz = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    let receipt = Receipt {
        job_id: req.job_id.clone(),
        node_id: klucz.public().to_hex(),
        model_hash: model_hash.to_string(),
        runtime: "vllm-openai/1".to_string(),
        precision: Precision::Fp16,
        // M2.5.1b: do policzenia poza API vLLM (runner + hidden states).
        activation_hash: "toploc:NIE_POLICZONY".to_string(),
        output_digest: simon_core::content_digest(&serde_json::json!({
            "job_id": req.job_id,
            "tokens_out": tokens_out,
            "ttft_ms": ttft_ms,
            "gen_ms": gen_ms,
        }))
        .unwrap_or_else(|_| "digest-blad".to_string()),
        started_at_us: teraz,
        finished_at_us: teraz,
        signer: klucz.public(),
        signature: None,
    };

    match receipt.sign(klucz) {
        Ok(podpisany) => serde_json::to_string(&podpisany)
            .unwrap_or_else(|e| format!("{{\"blad\":\"serializacja: {e}\"}}")),
        Err(e) => {
            eprintln!("[node] BŁĄD podpisu receiptu: {e}");
            "{}".to_string()
        }
    }
}

fn blad(req: &PromptRequest, kod: &str, opis: &str) -> PromptReply {
    blad_ctx(req, kod, opis, None, None)
}

/// Jak `blad`, ale dla `ctx_ponad_limit` dołącza STRUKTURALNE dane (nie do
/// wyparsowania z tekstu) — wołający (np. przyszły map-reduce w agent.rs)
/// dostaje realną liczbę tokenów i limit node'a bez zgadywania formatu `opis`.
fn blad_ctx(
    req: &PromptRequest,
    kod: &str,
    opis: &str,
    realny_ctx: Option<u32>,
    limit_ctx: Option<u32>,
) -> PromptReply {
    eprintln!("[node] BŁĄD {kod}: {opis}");
    PromptReply::Blad(PromptError {
        v: 1,
        order_id: req.order_id.clone(),
        kod: kod.to_string(),
        opis: opis.to_string(),
        realny_ctx,
        limit_ctx,
    })
}

