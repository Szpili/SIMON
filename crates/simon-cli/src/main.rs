#![allow(clippy::doc_overindented_list_items, clippy::unwrap_or_default)]
//! SIMON - CLI binary: `simon --role agent|coord|node` (M1.2).
//!
//! Three roles:
//! - `node`  - subscribes to prompt channel, executes model, returns result + receipt
//! - `agent` - broadcasts order, selects acceptance, sends prompt via p2p, verifies receipt
//! - `coord` - accepts orders and broadcasts `OrderAccepted`

use std::process::ExitCode;

mod agent;
mod common;
mod coord;
mod mapreduce;
mod node;

const USAGE: &str = r#"
SIMON — distributed inference (M1)

USAGE:
    simon --role <agent|coord|node> [OPTIONS]

ROLES:
    node     computes models for the network (worker)
    agent    orders work (client / harness)
    coord    accepts orders and broadcasts acceptance

OPTIONS:
    --listen <multiaddr>     listen address, e.g. /ip4/0.0.0.0/tcp/9001
    --bootstrap <multiaddr>  bootstrap peer
    --model-url <url>        [node] endpoint for vLLM, e.g. http://127.0.0.1:18020
                             (API key via SIMON_MODEL_API_KEY environment variable)
    --rate-per-peer <n>      [node] orders/s accepted from a single peer (default 16);
                             excess returns rate_limit error. 0 = unlimited.
    --max-jobs <n>           [node] concurrent prompts to process (default 1).
                             Excess queued (4 per slot), beyond that returns node_busy.
    --model-hash <hash>      [node] declared model / [agent] requested model
    --prompt <text|->        [agent] prompt to compute; in --file mode this is the
                             query/instruction applied to each chunk. '-' = stdin.
    --request-file <file|->  [agent] chat mode: OpenAI request JSON with messages/tools;
                             '-' = stdin.
    --file <path>            [agent] M3.x: file exceeding node limit —
                             enables chunking + map-reduce.
    --chunk-tokens <n>       [agent] token budget per chunk in --file (default 4000)
    --map-max-tokens <n>     [agent] output limit per chunk in --file (default 200)
    --then-bootstrap <addr>  [agent] B2 chain: after verifying STAGE A output,
                             send --then-prompt + A's output (as DATA) to this node
    --then-model-hash <m>    [agent] model requested for stage B (default: same as --model-hash)
    --then-prompt <text>     [agent] stage B instruction (what to do with stage A output)
    --max-tokens <n>         [agent] token limit (default 128)
    --fee <n>                [agent] THINK fee (default 1)
    --json                   [agent] single JSON object on stdout (result, measurement,
                             raw receipt, verification verdict)
    --identity-file <file>   [agent] M5.2a: client persistent identity file.
    --registry <file>        [agent] M5.2: append completed work to local work registry.
    --expect-output <file>   [with --verify-receipt] verify receipt describes this content.
    --tokens <file>          [with --verify-receipt] M1: JSON file with input/state binding.
    --audyt <file>           [with --verify-receipt] M3: JSON file with execution audit.
    --expect-signer <hex>    [agent and --verify-receipt] receipt must be signed by this node key.
    --verify-receipt <file>  verify receipt offline ('-' = stdin).

SUBCOMMAND (alias for --verify-receipt):
    simon verify <receipt> [--output <file>] [--job-id <id>] [--model <m>] [--signer <hex>] [--json]
    --key-file <file>        [node] load key seed from file.
    --key <hex>              [node] persistent Ed25519 key (64 hex = 32 B seed).
    -h, --help               print help

EXAMPLES:
    # node with vLLM
    simon --role node --listen /ip4/0.0.0.0/tcp/9001 --model-url http://127.0.0.1:18020 --model-hash qwen3.8-27b

    # agent (client) connecting to node
    simon --role agent --bootstrap /ip4/<IP_NODE>/tcp/9001 --prompt "compute 2+2"

    # offline verification
    simon verify examples/receipt.json --output examples/response.txt

NOTE (RULES #1): prompt travels over point-to-point channel (/simon/prompt/1),
never gossipsub. Only metadata travels over gossipsub.
"#;

/// Process role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rola {
    Agent,
    Coord,
    Node,
}

/// Parsed options.
#[derive(Debug, Clone)]
pub struct Opcje {
    pub rola: Rola,
    pub listen: Option<String>,
    pub bootstrap: Vec<String>,
    pub model_url: Option<String>,
    pub model_hash: Option<String>,
    pub prompt: Option<String>,
    pub max_tokens: u32,
    /// Concurrent prompts to process.
    pub max_jobs: usize,
    /// [node] Rate limit orders/s per peer (0 = unlimited).
    pub rate_per_peer: f64,
    /// Chat mode request file or '-' (stdin).
    pub request_file: Option<String>,
    pub fee: u64,
    /// Persistent node key (hex, 32 B seed).
    pub key: Option<String>,
    /// File exceeding node limit - enables chunking + map-reduce.
    pub file: Option<String>,
    pub chunk_tokens: u32,
    pub map_max_tokens: u32,
    /// B2 chain: pass verified stage A output to stage B.
    pub then_bootstrap: Vec<String>,
    pub then_model_hash: Option<String>,
    pub then_prompt: Option<String>,
    /// Single JSON output object.
    pub json: bool,
    /// Path to node key file.
    pub key_file: Option<String>,
    /// Offline receipt verification path or '-' (stdin).
    pub verify_receipt: Option<String>,
    pub expect_job_id: Option<String>,
    pub expect_model: Option<String>,
    /// File with expected output content.
    pub expect_output: Option<String>,
    /// M5.2: local work registry file (JSON Lines append-only).
    pub registry: Option<String>,
    /// M5.2a: client persistent identity file.
    pub identity_file: Option<String>,
    /// M1 token binding file.
    pub tokens: Option<String>,
    /// M3 execution audit file.
    pub audyt: Option<String>,
    /// Expected signer public key (hex).
    pub expect_signer: Option<String>,
}

impl Opcje {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut rola: Option<Rola> = None;
        let mut listen = None;
        let mut bootstrap = Vec::new();
        let mut model_url = None;
        let mut model_hash = None;
        let mut prompt = None;
        let mut max_tokens = 128u32;
        let mut max_jobs = 1usize;
        let mut rate_per_peer = 16.0f64;
        let mut request_file = None;
        let mut fee = 1u64;
        let mut key = None;
        let mut file = None;
        let mut chunk_tokens = 4000u32;
        let mut map_max_tokens = 200u32;
        let mut then_bootstrap = Vec::new();
        let mut then_model_hash = None;
        let mut then_prompt = None;
        let mut json = false;
        let mut key_file = None;
        let mut verify_receipt = None;
        let mut expect_job_id = None;
        let mut expect_model = None;
        let mut expect_output = None;
        let mut registry = None;
        let mut identity_file = None;
        let mut tokens = None;
        let mut audyt = None;
        let mut expect_signer = None;

        let mut i = 0;
        while i < args.len() {
            let arg = args[i].as_str();
            let wartosc = |i: &mut usize, nazwa: &str| -> Result<String, String> {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .ok_or_else(|| format!("missing value for {nazwa}"))
            };
            match arg {
                "-h" | "--help" => return Err(USAGE.to_string()),
                "--role" => {
                    let v = wartosc(&mut i, "--role")?;
                    rola = Some(match v.as_str() {
                        "agent" => Rola::Agent,
                        "coord" => Rola::Coord,
                        "node" => Rola::Node,
                        inny => return Err(format!("unknown role: {inny}")),
                    });
                }
                "--listen" => listen = Some(wartosc(&mut i, "--listen")?),
                "--bootstrap" => bootstrap.push(wartosc(&mut i, "--bootstrap")?),
                "--model-url" => model_url = Some(wartosc(&mut i, "--model-url")?),
                "--model-hash" => model_hash = Some(wartosc(&mut i, "--model-hash")?),
                "--prompt" => prompt = Some(wartosc(&mut i, "--prompt")?),
                "--request-file" => request_file = Some(wartosc(&mut i, "--request-file")?),
                "--max-tokens" => {
                    let v = wartosc(&mut i, "--max-tokens")?;
                    max_tokens = v.parse().map_err(|_| format!("invalid --max-tokens: {v}"))?;
                }
                "--max-jobs" => {
                    let v = wartosc(&mut i, "--max-jobs")?;
                    max_jobs = v.parse().map_err(|_| format!("invalid --max-jobs: {v}"))?;
                    if max_jobs == 0 {
                        return Err("--max-jobs must be > 0".to_string());
                    }
                }
                "--rate-per-peer" => {
                    let v = wartosc(&mut i, "--rate-per-peer")?;
                    rate_per_peer = v.parse().map_err(|_| format!("invalid --rate-per-peer: {v}"))?;
                    if !(rate_per_peer >= 0.0 && rate_per_peer.is_finite()) {
                        return Err("--rate-per-peer must be >= 0".to_string());
                    }
                }
                "--fee" => {
                    let v = wartosc(&mut i, "--fee")?;
                    fee = v.parse().map_err(|_| format!("invalid --fee: {v}"))?;
                }
                "--key" => key = Some(wartosc(&mut i, "--key")?),
                "--file" => file = Some(wartosc(&mut i, "--file")?),
                "--chunk-tokens" => {
                    let v = wartosc(&mut i, "--chunk-tokens")?;
                    chunk_tokens = v.parse().map_err(|_| format!("invalid --chunk-tokens: {v}"))?;
                    if chunk_tokens == 0 {
                        return Err("--chunk-tokens must be > 0".to_string());
                    }
                }
                "--map-max-tokens" => {
                    let v = wartosc(&mut i, "--map-max-tokens")?;
                    map_max_tokens = v.parse().map_err(|_| format!("invalid --map-max-tokens: {v}"))?;
                }
                "--then-bootstrap" => then_bootstrap.push(wartosc(&mut i, "--then-bootstrap")?),
                "--then-model-hash" => then_model_hash = Some(wartosc(&mut i, "--then-model-hash")?),
                "--then-prompt" => then_prompt = Some(wartosc(&mut i, "--then-prompt")?),
                "--json" => json = true,
                "--key-file" => key_file = Some(wartosc(&mut i, "--key-file")?),
                "--verify-receipt" => verify_receipt = Some(wartosc(&mut i, "--verify-receipt")?),
                "--expect-job-id" => expect_job_id = Some(wartosc(&mut i, "--expect-job-id")?),
                "--expect-model" => expect_model = Some(wartosc(&mut i, "--expect-model")?),
                "--expect-output" => expect_output = Some(wartosc(&mut i, "--expect-output")?),
                "--registry" => registry = Some(wartosc(&mut i, "--registry")?),
                "--identity-file" => identity_file = Some(wartosc(&mut i, "--identity-file")?),
                "--tokens" => tokens = Some(wartosc(&mut i, "--tokens")?),
                "--audyt" => audyt = Some(wartosc(&mut i, "--audyt")?),
                "--expect-signer" | "--signer" => expect_signer = Some(wartosc(&mut i, arg)?),
                inny => return Err(format!("unknown argument: {inny}")),
            }
            i += 1;
        }

        let rola = match rola {
            Some(r) => r,
            // --verify-receipt works offline, role is immaterial
            None if verify_receipt.is_some() => Rola::Agent,
            None => return Err("missing --role (agent|coord|node)".into()),
        };
        Ok(Self {
            rola,
            listen,
            bootstrap,
            model_url,
            model_hash,
            prompt,
            max_tokens,
            max_jobs,
            rate_per_peer,
            request_file,
            fee,
            key,
            file,
            chunk_tokens,
            map_max_tokens,
            then_bootstrap,
            then_model_hash,
            then_prompt,
            json,
            key_file,
            verify_receipt,
            expect_job_id,
            expect_model,
            expect_output,
            registry,
            identity_file,
            tokens,
            audyt,
            expect_signer,
        })
    }
}

#[derive(serde::Deserialize)]
struct PlikTokenow {
    tokenizer_hash: String,
    client_nonce: String,
    prompt_token_ids: Vec<u32>,
    output_token_ids: Vec<u32>,
}

/// `--verify-receipt`: verifies a provided receipt offline.
fn weryfikuj_offline(opcje: &Opcje, zrodlo: &str) -> ExitCode {
    use std::io::Read;
    let tekst = if zrodlo == "-" {
        let mut b = String::new();
        match std::io::stdin().read_to_string(&mut b) {
            Ok(_) => b,
            Err(e) => {
                eprintln!("ERROR: cannot read stdin: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        match std::fs::read_to_string(zrodlo) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("ERROR: cannot read {zrodlo}: {e}");
                return ExitCode::FAILURE;
            }
        }
    };

    let r: simon_core::receipt::Receipt = match serde_json::from_str(tekst.trim()) {
        Ok(r) => r,
        Err(e) => {
            if opcje.json {
                println!("{}", serde_json::json!({
                    "ok": false, "parses": false, "parsuje_sie": false, "reason": e.to_string(), "powod": e.to_string()}));
            } else {
                eprintln!("INVALID: not a receipt ({e})");
            }
            return ExitCode::FAILURE;
        }
    };

    let podpis_ok = r.verify_self().is_ok();
    let tresc_ok = match opcje.expect_output.as_deref() {
        Some(sciezka) => match std::fs::read_to_string(sciezka) {
            Ok(t) => Some(r.matches_output(&t)),
            Err(e) => {
                eprintln!("ERROR: cannot read --expect-output {sciezka}: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let job_ok = opcje.expect_job_id.as_ref().map(|j| *j == r.job_id);
    let model_ok = opcje.expect_model.as_ref().map(|m| *m == r.model_hash);
    let signer_ok = match opcje.expect_signer.as_deref() {
        Some(hex_pk) => {
            let zgadza_sie = r.signer.to_hex().eq_ignore_ascii_case(hex_pk.trim());
            if !zgadza_sie && !opcje.json {
                eprintln!(
                    "ERROR: receipt signer ({}) does not match expected ({})",
                    r.signer.to_hex(),
                    hex_pk.trim()
                );
            }
            Some(zgadza_sie)
        }
        None => None,
    };

    // M1: optional input/state token binding.
    let m1_wynik = match opcje.tokens.as_deref() {
        Some(sciezka) => {
            let tekst = match std::fs::read_to_string(sciezka) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("ERROR: cannot read --tokens {sciezka}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            match serde_json::from_str::<PlikTokenow>(&tekst) {
                Ok(p) => Some(r.verify_m1(
                    &p.tokenizer_hash,
                    &p.client_nonce,
                    &p.prompt_token_ids,
                    &p.output_token_ids,
                )),
                Err(e) => {
                    eprintln!("ERROR: invalid format --tokens {sciezka}: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        None => None,
    };
    let m1_ok = m1_wynik.as_ref().map(|w| w.is_ok());

    // M3: optional containment execution audit.
    let m3_wynik = match opcje.audyt.as_deref() {
        Some(sciezka) => {
            let tekst = match std::fs::read_to_string(sciezka) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("ERROR: cannot read --audyt {sciezka}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            match simon_core::m3::evaluate_json(&tekst, &simon_core::m3::M3Policy::default()) {
                Ok(w) => Some(w),
                Err(e) => {
                    eprintln!("ERROR: invalid format --audyt {sciezka}: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        None => None,
    };
    let m3_ok = m3_wynik.as_ref().map(|w| w.ok);

    let ok = podpis_ok
        && job_ok != Some(false)
        && model_ok != Some(false)
        && signer_ok != Some(false)
        && tresc_ok != Some(false)
        && m1_ok != Some(false)
        && m3_ok != Some(false);

    if opcje.json {
        println!("{}", serde_json::json!({
            "ok": ok,
            "parses": true,
            "parsuje_sie": true,
            "signature_ok": podpis_ok,
            "podpis_ok": podpis_ok,
            "job_id_ok": job_ok,
            "model_ok": model_ok,
            "signer_ok": signer_ok,
            "content_ok": tresc_ok,
            "tresc_ok": tresc_ok,
            "level": r.level(),
            "m1_ok": m1_ok,
            "m3_ok": m3_ok,
            "prompt_tokens": r.prompt_tokens,
            "completion_tokens": r.completion_tokens,
            "job_id": r.job_id,
            "node_id": r.node_id,
            "model_hash": r.model_hash,
            "runtime": r.runtime,
            "signer": r.signer,
        }));
    } else {
        println!("Ed25519 signature : {}", if podpis_ok { "OK" } else { "INVALID" });
        if let Some(v) = job_ok { println!("job_id           : {}", if v { "matches" } else { "MISMATCH" }); }
        if let Some(v) = model_ok { println!("model_hash       : {}", if v { "matches" } else { "MISMATCH" }); }
        if let Some(v) = signer_ok { println!("signer           : {}", if v { "matches" } else { "MISMATCH — different node key than expected" }); }
        if let Some(v) = tresc_ok { println!("output content   : {}", if v { "matches digest" } else { "MISMATCH — content does not match" }); }
        if let Some(w) = m1_wynik.as_ref() {
            match w {
                Ok(()) => println!("M1 binding       : OK (tokenizer + nonce + prompt + output)"),
                Err(e) => println!("M1 binding       : FAILED — {e}"),
            }
        }
        if let Some(w) = m3_wynik.as_ref() {
            println!(
                "M3 audit         : {} (steps={}, in top-k={}, outside={}, worst rank={}, margin={:.4})",
                if w.ok { "OK" } else { "REJECTED" },
                w.steps, w.in_topk, w.outside_topk, w.worst_rank, w.worst_margin
            );
            if let Some(p) = &w.reason {
                println!("                   {p}");
            }
        }
        let pz = r.level();
        println!("proof level      : {pz}");
        if pz == 0 {
            println!(
                "NOTE             : level 0 = authorship/signature only; NO input/state binding (M1).\n                   \
                 Do not present as execution verification (docs/THREAT-MODEL.md)."
            );
        }
        println!("node             : {}", r.node_id);
        println!("verdict          : {}", if ok { "RECEIPT VALID" } else { "REJECTED" });
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Expands `simon verify` subcommand into `--verify-receipt` / `--expect-*` flags.
fn rozwin_podkomende(args: Vec<String>) -> Vec<String> {
    if args.first().map(String::as_str) != Some("verify") {
        return args;
    }
    let mut out: Vec<String> = Vec::new();
    let mut receipt: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        let a = args[i].clone();
        // Flag with value: `cel` is destination flag, `args[i+1]` is value.
        let z_wartoscia = |out: &mut Vec<String>, cel: &str, i: &mut usize| {
            out.push(cel.to_string());
            if let Some(v) = args.get(*i + 1) {
                out.push(v.clone());
                *i += 1;
            }
        };
        match a.as_str() {
            "--output" | "-o" => z_wartoscia(&mut out, "--expect-output", &mut i),
            "--job-id" => z_wartoscia(&mut out, "--expect-job-id", &mut i),
            "--model" => z_wartoscia(&mut out, "--expect-model", &mut i),
            "--signer" | "--expect-signer" => z_wartoscia(&mut out, "--expect-signer", &mut i),
            "--json" => out.push("--json".into()),
            "-h" | "--help" => out.push("--help".into()),
            _ if a == "-" || !a.starts_with('-') => {
                if receipt.is_none() {
                    receipt = Some(a);
                } else {
                    out.push(a); // extra positional arg
                }
            }
            _ => out.push(a),
        }
        i += 1;
    }

    let mut final_args: Vec<String> = Vec::new();
    match receipt {
        Some(r) => {
            final_args.push("--verify-receipt".into());
            final_args.push(r);
        }
        None if !out.iter().any(|s| s == "--help") => {
            final_args.push("--verify-receipt".into());
        }
        None => {}
    }
    final_args.extend(out);
    final_args
}

/// Evaluates M3 containment audit without receipt.
fn audyt_podkomenda(args: &[String]) -> ExitCode {
    let mut plik: Option<&str> = None;
    let mut level: u8 = 1;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--level" => {
                i += 1;
                level = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(1);
            }
            "-h" | "--help" => {
                println!(
                    "simon audit <file.json> [--level N]\n  \
                     evaluates M3 audit (steps: node_token vs top-k) with k=2/margin 0.25 policy;\n  \
                     decision: Pass / SoftFail / HardFail / NoGrounds"
                );
                return ExitCode::SUCCESS;
            }
            inny => plik = Some(inny),
        }
        i += 1;
    }
    let Some(sciezka) = plik else {
        eprintln!("ERROR: provide audit file: simon audit <file.json>");
        return ExitCode::FAILURE;
    };
    let tekst = match std::fs::read_to_string(sciezka) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ERROR: cannot read {sciezka}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match simon_core::m3::evaluate_json(&tekst, &simon_core::m3::M3Policy::default()) {
        Ok(w) => {
            let d = simon_core::m3::decide(&w, level, &simon_core::m3::M3Thresholds::default());
            println!(
                "audit      : steps={} in top-k={} outside-top-k={} worst rank={} margin={:.4}",
                w.steps, w.in_topk, w.outside_topk, w.worst_rank, w.worst_margin
            );
            println!("verdict M3 : {}", if w.ok { "OK" } else { "FAIL" });
            println!("decision   : {d:?} (receipt level {level})");
            if let Some(p) = &w.reason {
                println!("reason     : {p}");
            }
            if matches!(d, simon_core::m3::Decision::HardFail) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("ERROR: invalid audit: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `simon guard [--url U] [--stdin] "text"` - guard entrance gateway.
fn guard_podkomenda(args: &[String]) -> ExitCode {
    let mut url = "http://127.0.0.1:19301/guard".to_string();
    let mut tekst: Option<String> = None;
    let mut ze_stdin = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--url" => {
                i += 1;
                if let Some(u) = args.get(i) {
                    url = u.clone();
                }
            }
            "--stdin" => ze_stdin = true,
            "-h" | "--help" => {
                println!("simon guard [--url http://127.0.0.1:19301/guard] [--stdin] \"text\"");
                return ExitCode::SUCCESS;
            }
            inny => tekst = Some(inny.to_string()),
        }
        i += 1;
    }
    if ze_stdin {
        use std::io::Read;
        let mut s = String::new();
        if std::io::stdin().read_to_string(&mut s).is_err() {
            eprintln!("ERROR: cannot read stdin");
            return ExitCode::FAILURE;
        }
        tekst = Some(s);
    }
    let Some(t) = tekst else {
        eprintln!("ERROR: provide text or --stdin");
        return ExitCode::FAILURE;
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("ERROR: runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    rt.block_on(async move {
        let klient = reqwest::Client::new();
        match klient
            .post(&url)
            .json(&serde_json::json!({ "text": t }))
            .send()
            .await
        {
            Ok(odp) => {
                let v: serde_json::Value = odp.json().await.unwrap_or_default();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())
                );
                if v.get("ryzyko").and_then(|x| x.as_bool()).unwrap_or(false) {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("ERROR: guard unavailable at {url} ({e})");
                ExitCode::FAILURE
            }
        }
    })
}

fn main() -> ExitCode {
    let args0: Vec<String> = std::env::args().skip(1).collect();
    if args0.first().map(String::as_str) == Some("audyt") {
        return audyt_podkomenda(&args0);
    }
    if args0.first().map(String::as_str) == Some("guard") {
        return guard_podkomenda(&args0);
    }
    let args = rozwin_podkomende(args0);
    let opcje = match Opcje::parse(&args) {
        Ok(o) => o,
        Err(msg) => {
            if msg.starts_with('\n') {
                println!("{msg}");
                return ExitCode::SUCCESS;
            }
            eprintln!("ERROR: {msg}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    // Offline receipt verification without runtime or networking.
    if let Some(zrodlo) = opcje.verify_receipt.clone() {
        return weryfikuj_offline(&opcje, &zrodlo);
    }

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("ERROR: cannot start runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    let wynik = rt.block_on(async {
        match opcje.rola {
            Rola::Node => node::run(&opcje).await,
            Rola::Agent => agent::run(&opcje).await,
            Rola::Coord => coord::run(&opcje).await,
        }
    });

    match wynik {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod testy_opcji {
    use super::*;

    #[test]
    fn prompt_dash_parses_like_any_other_value() {
        let args: Vec<String> = ["--role", "agent", "--bootstrap", "/ip4/1.2.3.4/tcp/9001", "--prompt", "-"]
            .iter().map(|s| s.to_string()).collect();
        let opcje = Opcje::parse(&args).expect("parsing must succeed");
        assert_eq!(opcje.prompt, Some("-".to_string()));

        let args_tekst: Vec<String> = ["--role", "agent", "--bootstrap", "/ip4/1.2.3.4/tcp/9001",
                                       "--prompt", "compute 2+2"]
            .iter().map(|s| s.to_string()).collect();
        let opcje_tekst = Opcje::parse(&args_tekst).expect("parsing must succeed");
        assert_eq!(opcje_tekst.prompt, Some("compute 2+2".to_string()));
    }
}

#[cfg(test)]
mod testy_cli {
    use super::*;
    use simon_core::crypto::Keypair;
    use simon_core::receipt::{Precision, Receipt};

    #[test]
    fn test_opcje_parse_expect_signer() {
        let args = vec![
            "--verify-receipt".to_string(),
            "receipt.json".to_string(),
            "--expect-signer".to_string(),
            "0cb2fad76b7a53caf66892d2e7f56cb4c2d70c04929fadc50a802ba86325d17d".to_string(),
        ];
        let opcje = Opcje::parse(&args).expect("parsuje --expect-signer");
        assert_eq!(
            opcje.expect_signer.as_deref(),
            Some("0cb2fad76b7a53caf66892d2e7f56cb4c2d70c04929fadc50a802ba86325d17d")
        );
    }

    #[test]
    fn test_rozwin_podkomende_verify_signer() {
        let args = vec![
            "verify".to_string(),
            "receipt.json".to_string(),
            "--signer".to_string(),
            "0cb2fad76b7a53caf66892d2e7f56cb4c2d70c04929fadc50a802ba86325d17d".to_string(),
        ];
        let expanded = rozwin_podkomende(args);
        assert!(expanded.contains(&"--expect-signer".to_string()));
        let opcje = Opcje::parse(&expanded).expect("parsuje rozszerzone opcje verify");
        assert_eq!(
            opcje.expect_signer.as_deref(),
            Some("0cb2fad76b7a53caf66892d2e7f56cb4c2d70c04929fadc50a802ba86325d17d")
        );
    }

    #[test]
    fn test_weryfikuj_offline_odrzuca_zly_signer() {
        let klucz1 = Keypair::from_seed(&[1u8; 32]);
        let klucz2 = Keypair::from_seed(&[2u8; 32]);
        let r = Receipt {
            job_id: "j1".into(),
            node_id: klucz1.public().to_hex(),
            model_hash: "m1".into(),
            runtime: "vllm/1".into(),
            precision: Precision::Fp16,
            activation_hash: "act".into(),
            prompt_tokens: 1,
            completion_tokens: 1,
            output_digest: simon_core::receipt::output_digest_for("j1", "tekst").unwrap(),
            started_at_us: 100,
            finished_at_us: 200,
            signer: klucz1.public(),
            signature: None,
            bindings: None,
        };
        let signed = r.sign(&klucz1).unwrap();
        let sciezka = std::env::temp_dir().join(format!(
            "simon_test_receipt_{}_{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&sciezka, serde_json::to_string(&signed).unwrap()).unwrap();

        // Zgodny signer: ExitCode::SUCCESS
        let mut opcje = Opcje::parse(&[
            "--verify-receipt".into(),
            sciezka.to_str().unwrap().into(),
            "--expect-signer".into(),
            klucz1.public().to_hex(),
        ])
        .unwrap();
        assert_eq!(
            weryfikuj_offline(&opcje, sciezka.to_str().unwrap()),
            ExitCode::SUCCESS
        );

        // Niezgodny signer: ExitCode::FAILURE
        opcje.expect_signer = Some(klucz2.public().to_hex());
        assert_eq!(
            weryfikuj_offline(&opcje, sciezka.to_str().unwrap()),
            ExitCode::FAILURE
        );

        let _ = std::fs::remove_file(&sciezka);
    }
}
