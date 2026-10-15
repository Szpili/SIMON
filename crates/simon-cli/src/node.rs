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
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;

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
    let (model_z_backendu, max_ctx) =
        odczytaj_moje_fakty(&model_url, opcje.model_hash.as_deref()).await;
    if max_ctx == 0 {
        eprintln!("[node] UWAGA: nie mogę odczytać max_model_len — nie będę odrzucał zleceń");
    } else {
        println!("[node] max_ctx (z /v1/models) = {max_ctx}");
    }
    // C-gate: co node DEKLARUJE (idzie do rejestru → filtrowanie przydziału).
    // `--model-hash` zostaje jako etykieta, ale prawda pochodzi z vLLM.
    // Gdy backend potwierdził żądany model — ogłaszamy jego `id`. Gdy nie
    // potwierdził, ogłaszamy to, co sami deklarujemy, ale MÓWIMY o tym głośno:
    // cicha etykieta bez pokrycia jest gorsza niż brak etykiety.
    let model_deklarowany = match model_z_backendu {
        Some(m) => m,
        None => {
            eprintln!(
                "[node] UWAGA: ogłaszam '{model_hash}' BEZ potwierdzenia od backendu"
            );
            model_hash.clone()
        }
    };
    println!("[node] deklaruję: model={model_deklarowany} max_ctx={max_ctx}");
    let runtime = wykryj_runtime(&model_url).await;
    println!("[node] model_url={model_url} model_hash={model_hash} runtime={runtime}");

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

    // Zadania liczymy POZA pętlą swarmu. Wcześniej `policz` był awaitowany
    // tu, w pętli: przez cały czas generowania swarm nie był pollowany, więc
    // połączenia, które przyszły w tym czasie, nie dostawały handlera, a ich
    // prompty ginęły bez śladu (zmierzone 2026-10-11: 4 agenty, joby ~15 s,
    // 2 prompty nigdy nie dotarły, agenci czekali do swojego timeoutu 300 s).
    let sloty = Arc::new(Semaphore::new(opcje.max_jobs));
    // ponytail: stała kolejka na slot; konfigurowalna, gdy ktoś zmierzy, że 4 to źle.
    // Zadania w kolejce czekają w ramach `REQUEST_TIMEOUT` protokołu (300 s),
    // więc głębsza kolejka i tak skończyłaby się timeoutem u klienta.
    let przyjecia = Arc::new(Semaphore::new(opcje.max_jobs * KOLEJKA_NA_SLOT));
    let (gotowe_tx, mut gotowe_rx) = tokio::sync::mpsc::unbounded_channel();
    let backend = Arc::new(Backend {
        model_url,
        model: model_deklarowany,
        runtime,
        klucz,
        max_ctx,
    });

    println!(
        "[node] czekam na prompty (punk-punkt, /simon/prompt/1), max_jobs={} kolejka={}...",
        opcje.max_jobs,
        opcje.max_jobs * KOLEJKA_NA_SLOT
    );

    loop {
        let zdarzenie = tokio::select! {
            Some((channel, reply)) = gotowe_rx.recv() => {
                if let Err(e) = swarm.behaviour_mut().prompt_rr.send_response(channel, reply) {
                    eprintln!("[node] nie mogę odesłać odpowiedzi (klient się rozłączył?): {e:?}");
                }
                continue;
            }
            z = swarm.select_next_some() => z,
        };
        match zdarzenie {
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
                    if let Some(reply) = sprawdz_wersje(&request) {
                        let _ = swarm.behaviour_mut().prompt_rr.send_response(channel, reply);
                        continue;
                    }
                    // Pozwolenie na przyjęcie żyje do końca zadania (także przy
                    // panice), więc licznik nie przecieka.
                    let Ok(przyjete) = przyjecia.clone().try_acquire_owned() else {
                        let reply = blad(
                            &request,
                            "node_zajety",
                            &format!(
                                "node zajęty: {} zadań w toku lub w kolejce — spróbuj później",
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
                        let reply = obsluz(&b, &request).await;
                        let _ = tx.send((channel, reply));
                    });
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

/// Ile zadań może czekać w kolejce na jeden slot `--max-jobs`, zanim node
/// odpowie `node_zajety`.
const KOLEJKA_NA_SLOT: usize = 4;

const WERSJA_PROMPTU: u16 = 1;

/// Co zadanie musi wiedzieć o backendzie — współdzielone między zadaniami.
struct Backend {
    model_url: String,
    model: String,
    runtime: String,
    klucz: Keypair,
    max_ctx: u32,
}

fn sprawdz_wersje(request: &PromptRequest) -> Option<PromptReply> {
    (request.v > WERSJA_PROMPTU).then(|| {
        blad(
            request,
            "wersja",
            &format!(
                "nieobsługiwana wersja formatu {} (node obsługuje do {})",
                request.v, WERSJA_PROMPTU
            ),
        )
    })
}

/// Jedno zlecenie: bramka kontekstu, potem liczenie. Biegnie w osobnym
/// zadaniu tokio, nigdy w pętli swarmu.
async fn obsluz(b: &Backend, request: &PromptRequest) -> PromptReply {
    if let Some(reply) = sprawdz_wersje(request) {
        return reply;
    }
    // C+1: agent deklaruje max_ctx przybliżeniem znakowym (~3,5 B/tok
    // dla polskiej prozy) — przy JSON/base64 błąd sięga ~2,5× w obie
    // strony (docs/KV-CACHE-spill-i-rozproszony-VRAM.md). Node MA
    // prompt i vLLM pod ręką, więc pyta o prawdziwą liczbę tokenów
    // zamiast ufać cudzemu szacunkowi. Gdy /tokenize nieosiągalny —
    // spadamy na deklarację agenta (lepiej przepuścić niż zablokować
    // wszystko przez chwilową awarię wywołania).
    let realny_ctx = odczytaj_realny_ctx(&b.model_url, &request.prompt).await;
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
    if b.max_ctx > 0 && ctx_do_bramki > b.max_ctx {
        eprintln!(
            "[node] ODRZUCAM: zlecenie żąda {} tokenów ctx, mam limit {}",
            ctx_do_bramki, b.max_ctx
        );
        blad_ctx(
            request,
            "ctx_ponad_limit",
            &format!(
                "zlecenie żąda {} tokenów kontekstu, ten node ma limit {} (model {})",
                ctx_do_bramki, b.max_ctx, b.model
            ),
            Some(ctx_do_bramki),
            Some(b.max_ctx),
        )
    } else {
        policz(&b.model_url, &b.model, &b.runtime, &b.klucz, request).await
    }
}

/// Liczy prompt przez API zgodne z OpenAI (`/v1/chat/completions`).
///
/// **Mierzy TTFT i czas generowania osobno** — M2.7 wymaga rozbicia pomiaru.
async fn policz(
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
        Err(e) => return blad(req, "client", &format!("nie mogę zbudować klienta HTTP: {e}")),
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
        Err(e) => return blad(req, "json", &format!("zła odpowiedź modelu: {e}")),
    };

    let (output, tresc_pusta) = wyjscie(&v, req.chat);
    if tresc_pusta {
        // NAPRAWA (audyt red-team Filipa, CWE-200): tu był fallback na
        // `reasoning`/`reasoning_content`, czyli wewnętrzne rozumowanie modelu
        // podpisywane jako ODPOWIEDŹ klienta. Rozumowanie nie jest odpowiedzią —
        // pusta treść wraca jako błąd, NIE jako podpisany receipt z czymś innym.
        let finish = v["choices"][0]["finish_reason"].as_str().unwrap_or("nieznany");
        return blad(req, "pusta_tresc", &format!(
            "model nie zwrócił treści odpowiedzi (finish_reason={finish}); limit tokenów mógł zostać \
             zużyty na wewnętrzne rozumowanie (reasoning), które nie jest odpowiedzią"));
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
        "[node] policzone: {} tokenów ({} znaków wyjścia), czas backendu={gen_ms}ms (zmierzony, obejmuje prefill+decode)",
        tokens_out,
        output.len()
    );
    // Pusta treść już wraca błędem powyżej (`pusta_tresc`) — tu `output` zawsze
    // ma coś do podpisania (tekst w trybie tekstowym, JSON wiadomości w czacie).

    // Receipt PRZED przeniesieniem `output` — podpisujemy dokladnie ten tekst,
    // ktory zaraz odeslemy.
    let receipt = zbuduj_podpisany_receipt(
        klucz,
        model_hash,
        runtime,
        req,
        &output,
        prompt_tokens,
        tokens_out,
        started_at_us,
        finished_at_us,
    );

    PromptReply::Ok(PromptResponse {
        v: 1,
        order_id: req.order_id.clone(),
        job_id: req.job_id.clone(),
        output,
        // M2.5.1: PODPISANY receipt (Ed25519). Klient weryfikuje bramki:
        // podpis ważny, `signer` == klucz node'a, job_id/model zgodne,
        // ORAZ (od 2026-09-18) że odcisk pokrywa TĘ treść.
        receipt,
        tokens_out,
        ttft_ms,
        gen_ms,
    })
}

/// Pola żądania czatu, które node przekazuje backendowi. Biała lista: reszta
/// (np. `stream`, `n`, `logprobs`) zmieniłaby kształt odpowiedzi albo koszt
/// bez wiedzy node'a.
const POLA_CZATU: &[&str] = &["tools", "tool_choice", "parallel_tool_calls", "temperature", "top_p", "stop"];

/// Ciało `/v1/chat/completions`. Tryb tekstowy: jeden prompt użytkownika.
/// Tryb czatu: wiadomości i pola z białej listy z JSON-a, który klient podpisał.
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

/// Co node podpisuje (`output`) + czy treść wynikowa jest PUSTA (sygnał dla
/// wołającego: odpowiedz błędem, nie podpisanym receiptem).
///
/// NAPRAWA (audyt red-team Filipa, CWE-200): tu był fallback `content` pusty
/// -> `reasoning`/`reasoning_content` w trybie tekstowym, czyli WEWNĘTRZNE
/// rozumowanie modelu (qwen3.8 i inne modele rozumujące potrafią zużyć cały
/// budżet tokenów na rozumowanie) podpisywane jako ODPOWIEDŹ klienta. Receipt
/// wiąże podpis z treścią — podpisanie reasoning jako `output` fałszywie
/// przedstawia je jako sprawdzony wynik. Pusta treść wraca teraz jako błąd
/// (`pusta_tresc` w `policz`), nigdy jako podstawiony tekst.
///
/// Tryb czatu: JSON wiadomości `{content, tool_calls?, finish_reason}`. Receipt
/// wiąże więc nazwy i argumenty narzędzi, a `finish_reason` nie da udawać, że
/// ucięte w połowie argumenty to kompletne wywołanie. Treść pusta + brak
/// `tool_calls` jest tu też sygnalizowana jako pusta (patrz wyżej).
fn wyjscie(v: &serde_json::Value, chat: bool) -> (String, bool) {
    let msg = &v["choices"][0]["message"];
    let surowy = msg["content"].as_str().unwrap_or("");
    let narzedzia = msg
        .get("tool_calls")
        .filter(|t| t.as_array().is_some_and(|a| !a.is_empty()));
    if !chat {
        let tekst = surowy.trim().to_string();
        return (tekst.clone(), tekst.is_empty());
    }
    // Czat: treść bajt w bajt jak od modelu (bez trim) — tylko ocena pustki po trim.
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
    // Pusta treść + brak tool_calls = nic, co klient mógłby uznać za odpowiedź.
    let pusta = tekst.is_empty() && narzedzia.is_none();
    if let Some(t) = narzedzia {
        m.insert("tool_calls".into(), t.clone());
    }
    m.insert("finish_reason".into(), v["choices"][0]["finish_reason"].clone());
    (serde_json::Value::Object(m).to_string(), pusta)
}

/// C: odczytuje od backendu DWA fakty o node: `id` modelu i `max_model_len`.
///
/// Node NIE konfiguruje tego ręcznie — backend jest źródłem prawdy o samym
/// sobie. `id` zasila `Capability.declared` (filtr przydziału po modelu),
/// `max_model_len` zasila bramkę kontekstu.
///
/// NAPRAWA (2026-09-18): wcześniej brano bezwarunkowo `data[0]`. Przy vLLM,
/// który serwuje jeden model, to działało. Przy backendzie wielomodelowym
/// (Ollama wystawiła sześć) node OGŁASZAŁ SIECI INNY MODEL, NIŻ FAKTYCZNIE
/// LICZY — koordynator routowałby po czymś, czego node nie serwuje, a receipt
/// niósłby jeszcze inną nazwę. Gdy `--model-hash` jest podany, szukamy TEGO
/// modelu na liście; gdy go nie ma, to nie jest drobiazg do zalogowania, tylko
/// powód, żeby nie wstawać.
async fn odczytaj_moje_fakty(model_url: &str, zadany: Option<&str>) -> (Option<String>, u32) {
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
    // llama.cpp nie podaje `max_model_len` w /v1/models. Kontekst JEDNEGO slotu
    // (`-c` podzielone przez `-np`) jest w /props. Bez tego bramka kontekstu była
    // wyłączona i przepełnienie wracało dopiero z backendu (pilot LIDAR, 2026-10-14).
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
/// idzie jako `Authorization: Bearer`. Ze zmiennej środowiskowej, nie z argumentu:
/// argumenty procesu widać w `ps` dla każdego użytkownika maszyny.
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

/// Który wpis z `/v1/models` opisuje NAS. Wydzielone z zapytania sieciowego,
/// żeby dało się to przetestować bez serwera — to jest miejsce, w którym node
/// ostatnio ogłaszał cudzy model.
fn wybierz_wpis(lista: &[serde_json::Value], zadany: Option<&str>) -> (Option<String>, u32) {
    let wpis = match zadany {
        // Szukamy DOKŁADNIE tego modelu, o którym mówimy, że go serwujemy.
        Some(cel) => lista.iter().find(|m| m["id"].as_str() == Some(cel)),
        // Bez `--model-hash`: `data[0]` jest jednoznaczne tylko przy jednym
        // modelu. Przy kilku każdy wybór byłby zgadywaniem.
        None => match lista.len() {
            1 => lista.first(),
            _ => {
                if lista.len() > 1 {
                    eprintln!(
                        "[node] UWAGA: backend serwuje {} modeli, a nie podano --model-hash \
                         — nie zgaduję, który ogłosić",
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

/// Jaki silnik NAPRAWDĘ liczy. Wcześniej receipt twierdził na sztywno
/// `vllm-openai/1`, także gdy pod spodem stała Ollama (złapane 2026-09-18 na
/// drugim węźle). Pole `runtime` decyduje o kwalifikacji do puli i o tym, czy
/// dwa wyniki są w ogóle porównywalne — podpisana nieprawda w tym miejscu
/// psuje dokładnie to, czemu SIMON ma zapobiegać.
///
/// Gdy nie umiemy rozstrzygnąć, wpisujemy `nieznany/1`. Uczciwe „nie wiem"
/// jest mniej szkodliwe niż pewna zmyślona etykieta.
async fn wykryj_runtime(model_url: &str) -> String {
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
    eprintln!("[node] UWAGA: nie rozpoznaję silnika pod {baza} — receipt powie 'nieznany'");
    "nieznany/1".to_string()
}

/// Pomocnik do wyciągania liczby tokenów z odpowiedzi endpointu `/tokenize`.
/// Obsługuje format vLLM (`count: N`) oraz llama-server (`tokens: [...]`).
pub(crate) fn wyciagnij_liczbe_tokenow(v: &serde_json::Value) -> Option<u64> {
    v.get("count")
        .and_then(|c| c.as_u64())
        .or_else(|| v.get("tokens").and_then(|t| t.as_array()).map(|a| a.len() as u64))
}

/// C+1: prawdziwa liczba tokenów promptu przez `/tokenize` vLLM / llama-server
/// (POST `{"prompt": ..., "content": ...}` -> `{"count": N}` lub `{"tokens": [...]}`).
/// `None` gdy backend nieosiągalny lub odpowiedź nie ma pola tokenów — WOŁAJĄCY
/// spada wtedy na deklarację agenta. Każda droga do `None` loguje PRZYCZYNĘ:
/// cichy fallback bez logu maskowałby realny błąd parsowania jako "nieosiągalny".
async fn odczytaj_realny_ctx(model_url: &str, prompt: &str) -> Option<u32> {
    let url = format!("{}/tokenize", model_url.trim_end_matches('/'));
    let klient = match klient_backendu()
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
        .json(&serde_json::json!({"prompt": prompt, "content": prompt}))
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
    match wyciagnij_liczbe_tokenow(&v) {
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
            eprintln!("[node] /tokenize: odpowiedź bez pola `count` ani `tokens`: {v}");
            None
        }
    }
}

/// Wykrywa precyzję modelu na podstawie jego nazwy/identyfikatora.
pub(crate) fn wykryj_precyzje(model_hash: &str) -> Precision {
    let lower = model_hash.to_ascii_lowercase();
    if lower.contains("bf16") {
        Precision::Bf16
    } else if lower.contains("fp8") || lower.contains("q8") {
        Precision::Fp8
    } else if lower.contains("fp4")
        || lower.contains("q4")
        || lower.contains("awq")
        || lower.contains("gptq")
        || lower.contains("int4")
    {
        Precision::Fp4
    } else if lower.contains("fp32") || lower.contains("f32") {
        Precision::Fp32
    } else {
        Precision::Fp16
    }
}

/// M2.5.1: buduje receipt i PODPISUJE go kluczem node'a.
///
/// `activation_hash` jest na razie jawnym placeholderem — TopLoc wymaga
/// hidden states z runnera (nie z API vLLM). Nie udajemy, że jest policzony.
#[allow(clippy::too_many_arguments)]
fn zbuduj_podpisany_receipt(
    klucz: &Keypair,
    model_hash: &str,
    runtime: &str,
    req: &PromptRequest,
    output: &str,
    prompt_tokens: u32,
    tokens_out: u32,
    started_at_us: u64,
    finished_at_us: u64,
) -> String {
    let receipt = Receipt {
        job_id: req.job_id.clone(),
        node_id: klucz.public().to_hex(),
        model_hash: model_hash.to_string(),
        runtime: runtime.to_string(),
        precision: wykryj_precyzje(model_hash),
        // M2.5.1b: do policzenia poza API vLLM (runner + hidden states).
        activation_hash: "toploc:NIE_POLICZONY".to_string(),
        // Odcisk TRESCI, nie metadanych. Metadane (czasy) i tak sa podpisane,
        // bo wchodza do odcisku calego receiptu.
        prompt_tokens,
        completion_tokens: tokens_out,
        output_digest: simon_core::receipt::odcisk_wyjscia(&req.job_id, output)
            .unwrap_or_else(|_| "digest-blad".to_string()),
        started_at_us,
        finished_at_us,
        signer: klucz.public(),
        signature: None,
        wiazania: None,
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
        // Regresja 2026-09-18: node ogłaszał `data[0]`, czyli CUDZY model.
        let (id, ctx) = wybierz_wpis(&lista(), Some("bielik:Q4"));
        assert_eq!(id.as_deref(), Some("bielik:Q4"));
        assert_eq!(ctx, 8192, "max_ctx ma pochodzić z TEGO wpisu, nie z pierwszego");
    }

    #[test]
    fn brak_zadanego_modelu_to_nie_jest_cichy_fallback() {
        let (id, ctx) = wybierz_wpis(&lista(), Some("model-ktorego-nie-ma"));
        assert!(id.is_none(), "nie wolno ogłosić modelu, którego backend nie ma");
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
        assert!(id.is_none(), "przy kilku modelach wybór data[0] był źródłem błędu");
    }
}

#[cfg(test)]
mod testy_wspolbieznosci {
    use crate::wspolne::{klucz_p2p_z_seeda, zbuduj_swarm};
    use simon_harness::request_response::{PromptReply, PromptRequest};
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const OPOZNIENIE: Duration = Duration::from_secs(2);

    /// Udawany backend: każde `chat/completions` trwa `OPOZNIENIE`, reszta
    /// (/v1/models, /tokenize, /version) odpowiada od razu.
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
                // Błąd wersji ma pierwszeństwo przed bramką kontekstu i parsowaniem czatu.
                backend.max_ctx = 1;
                req.max_ctx = 2;
                req.prompt = "nie json".into();
                let reply = tokio::time::timeout(Duration::from_secs(1), super::obsluz(&backend, &req))
                    .await
                    .expect("odrzucenie wersji nie czeka na backend");
                let PromptReply::Blad(err) = reply else {
                    panic!("oczekiwano błędu wersji");
                };
                assert_eq!(err.kod, "wersja");
                assert_eq!(err.order_id, req.order_id);
                assert_eq!(err.v, 1);
                assert_eq!(err.realny_ctx, None);
                assert_eq!(err.limit_ctx, None);
                assert!(err.opis.contains(&v.to_string()));
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
                let reply = super::obsluz(&backend, &req).await;
                assert!(matches!(reply, PromptReply::Ok(_)), "{reply:?}");
            }
        }
    }

    /// Regresja 2026-10-11: node liczył prompt W pętli swarmu, więc N
    /// równoległych agentów było obsługiwanych po kolei (a przy dłuższych
    /// jobach część promptów ginęła). Przy `--max-jobs 4` cztery prompty
    /// mają się liczyć naraz: ~1× opóźnienie backendu, nie 4×.
    #[tokio::test(flavor = "multi_thread")]
    async fn cztery_prompty_licza_sie_naraz() {
        let backend = falszywy_backend().await;
        let wolny = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = wolny.local_addr().unwrap().port();
        drop(wolny);
        let seed = [7u8; 32];
        let peer = klucz_p2p_z_seeda(&seed).public().to_peer_id();
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
        tokio::spawn(async move { super::uruchom(&opcje).await });
        tokio::time::sleep(Duration::from_secs(1)).await;

        let adres = format!("/ip4/127.0.0.1/tcp/{port}/p2p/{peer}");
        let start = Instant::now();
        let agenci: Vec<_> = (0..4)
            .map(|i| {
                let adres = adres.clone();
                tokio::spawn(async move {
                    let mut swarm = zbuduj_swarm(&[], &[adres], None).unwrap();
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
                    crate::agent::wyslij_i_czekaj(&mut swarm, peer, req).await
                })
            })
            .collect();
        for a in agenci {
            let (reply, _) = a.await.unwrap().expect("agent dostał odpowiedź");
            assert!(matches!(reply, PromptReply::Ok(_)), "{reply:?}");
        }
        let czas = start.elapsed();
        assert!(
            czas < OPOZNIENIE * 3,
            "4 prompty trwały {czas:?} — liczone po kolei, nie naraz"
        );
    }
}

#[cfg(test)]
mod testy_czatu {
    use super::{cialo_zapytania, wyjscie};
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
        assert!(b.get("stream").is_none() && b.get("n").is_none(), "spoza białej listy: {b}");
        assert!(cialo_zapytania("m", &req(r#"{"messages": []}"#, true)).is_err());
        assert!(cialo_zapytania("m", &req("nie json", true)).is_err());
        // tryb tekstowy bez zmian: prompt jako jedna wiadomość użytkownika
        let t = cialo_zapytania("m", &req("nie json", false)).unwrap();
        assert_eq!(t["messages"][0]["content"], "nie json");
    }

    #[test]
    fn czat_podpisuje_wywolania_narzedzi_i_finish_reason() {
        let v = json!({"choices": [{"finish_reason": "tool_calls", "message": {"content": null,
            "tool_calls": [{"id": "c1", "type": "function",
                            "function": {"name": "read_file", "arguments": "{\"path\":\"a.rs\"}"}}]}}]});
        let (out, _) = wyjscie(&v, true);
        let m: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(m["content"].is_null());
        assert_eq!(m["tool_calls"][0]["function"]["arguments"], "{\"path\":\"a.rs\"}");
        assert_eq!(m["finish_reason"], "tool_calls");
        // ucięte wywołanie zostaje ucięte: finish_reason=length jest w podpisanym wyniku
        let v2 = json!({"choices": [{"finish_reason": "length", "message": {"content": "abc"}}]});
        assert_eq!(serde_json::from_str::<serde_json::Value>(&wyjscie(&v2, true).0).unwrap()["finish_reason"], "length");
        // tryb tekstowy: sam tekst, jak dotąd
        assert_eq!(wyjscie(&v2, false).0, "abc");
        // NAPRAWA (audyt red-team Filipa, CWE-200): rozumowanie NIGDY nie
        // zastępuje pustej treści — ani w trybie tekstowym, ani w czacie.
        // Pusta treść musi wrócić jako "pusta" (błąd u wołającego w `policz`),
        // nie jako reasoning podstawiony za odpowiedź.
        let v3 = json!({"choices": [{"finish_reason": "length", "message": {"content": "", "reasoning": "myślę"}}]});
        let (tekst3, pusta3) = wyjscie(&v3, false);
        assert_eq!(tekst3, "", "reasoning nie może stać się podpisaną odpowiedzią");
        assert!(pusta3, "pusta treść musi być zgłoszona jako pusta");
        let (out3_chat, pusta3_chat) = wyjscie(&v3, true);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&out3_chat).unwrap()["content"], "");
        assert!(pusta3_chat, "pusta treść w czacie bez tool_calls jest też pusta");
    }

    #[test]
    fn czat_z_narzedziami_nie_jest_pusty_mimo_content_null() {
        // tool_calls to prawdziwa odpowiedź nawet bez `content` — nie wolno
        // tego traktować jak "brak odpowiedzi" (patrz `pusta` w `wyjscie`).
        let v = json!({"choices": [{"finish_reason": "tool_calls", "message": {"content": null,
            "tool_calls": [{"id": "c1", "type": "function",
                            "function": {"name": "ls", "arguments": "{}"}}]}}]});
        let (_, pusta) = wyjscie(&v, true);
        assert!(!pusta, "tool_calls bez content to nie 'pusta odpowiedź'");
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
        let json_receipt = zbuduj_podpisany_receipt(
            &klucz, "gemma-4", "llama.cpp/1", &req, "odpowiedz", 10, 5, started, finished,
        );
        let r: Receipt = serde_json::from_str(&json_receipt).unwrap();
        assert_eq!(r.started_at_us, started);
        assert_eq!(r.finished_at_us, finished);
        assert!(r.started_at_us <= r.finished_at_us);
        assert!(r.verify_self().is_ok());
    }

    #[test]
    fn test_wykryj_precyzje_z_nazwy_modelu() {
        assert_eq!(wykryj_precyzje("gemma-4-26b-a4b-it-bf16"), Precision::Bf16);
        assert_eq!(wykryj_precyzje("gemma-4-26b-a4b-it-qat-q4_0"), Precision::Fp4);
        assert_eq!(wykryj_precyzje("qwen-q8_0"), Precision::Fp8);
        assert_eq!(wykryj_precyzje("model-awq"), Precision::Fp4);
        assert_eq!(wykryj_precyzje("model-gptq"), Precision::Fp4);
        assert_eq!(wykryj_precyzje("model-int4"), Precision::Fp4);
        assert_eq!(wykryj_precyzje("model-fp8"), Precision::Fp8);
        assert_eq!(wykryj_precyzje("model-fp32"), Precision::Fp32);
        assert_eq!(wykryj_precyzje("model-f32"), Precision::Fp32);
        assert_eq!(wykryj_precyzje("standard-model"), Precision::Fp16);
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



