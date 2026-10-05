//! SIMON — binarka: `simon --role agent|coord|node` (M1.2).
//!
//! Trzy role:
//! - `node`  — subskrybuje kanał promptu, liczy modelem i zwraca wynik + receipt
//! - `agent` — rozgłasza zlecenie, wybiera przyjęcie, wysyła prompt kanałem
//!             punkt-punkt, weryfikuje receipt lokalnie
//! - `coord` — przyjmuje zlecenia (podpisane, M0.1) i rozgłasza `OrderAccepted`

use std::process::ExitCode;

mod agent;
mod coord;
mod mapreduce;
mod node;
mod wspolne;

const USAGE: &str = r#"
SIMON — rozproszona inferencja (M1)

UŻYCIE:
    simon --role <agent|coord|node> [OPCJE]

ROLE:
    node     liczy modele dla sieci (worker)
    agent    zleca pracę (klient / harness)
    coord    przyjmuje zlecenia i rozgłasza przyjęcie

OPCJE:
    --listen <multiaddr>     nasłuch, np. /ip4/0.0.0.0/tcp/9001
    --bootstrap <multiaddr>  peer startowy (inny niż własny adres)
    --model-url <url>        [node] endpoint vLLM, np. http://127.0.0.1:18020
    --model-hash <hash>      [node] deklarowany model / [agent] model, o który prosisz
    --prompt <tekst>         [agent] prompt do policzenia; w trybie --file
                             to PYTANIE/instrukcja stosowana do każdego kawałka
    --file <ścieżka>         [agent] M3.x: plik większy niż limit node'a —
                             włącza chunkowanie + map-reduce (sekwencyjnie,
                             hierarchiczna redukcja). Patrz PRZYKŁADY.
    --chunk-tokens <n>       [agent] budżet tokenów na kawałek w --file
                             (domyślnie 4000; konserwatywny margines wg
                             najgęstszego zmierzonego materiału, base64)
    --map-max-tokens <n>     [agent] limit wyjścia NA KAWAŁEK w --file
                             (domyślnie 200 — trzyma reduce w ryzach)
    --then-bootstrap <addr>  [agent] B2: łańcuch — po zweryfikowaniu wyniku
                             ETAPU A, wyślij --then-prompt + wynik A (jako
                             DANE) do TEGO node'a (może inny model/karta)
    --then-model-hash <m>    [agent] model żądany na etapie B (domyślnie
                             jak --model-hash)
    --then-prompt <tekst>    [agent] instrukcja etapu B (co zrobić z
                             wynikiem etapu A)
    --max-tokens <n>         [agent] limit tokenów (domyślnie 128; w trybie
                             --file dotyczy rund redukcji, nie kawałków map)
    --fee <n>                [agent] opłata THINK (domyślnie 1)
    --json                   [agent] jeden obiekt JSON na stdout (wynik,
                             pomiar, surowy receipt, werdykt weryfikacji)
                             zamiast wydruku dla człowieka — dla demo/integracji
    --identity-file <plik>   [agent] M5.2a: plik trwałej tożsamości klienta.
                             Domyślnie katalog danych użytkownika. Sekretu NIE
                             podaje się w wierszu poleceń — byłby w `ps`.
    --rejestr <plik>         [agent] M5.2: dopisz wykonaną pracę do lokalnego
                             metrycznika (JSON Lines). NIE portfel, NIE saldo —
                             surowe liczniki, czasy mierzone u klienta i poziom
                             weryfikacji. Wykrywa powtórzenie receiptu.
    --expect-output <plik>   [z --verify-receipt] sprawdź, czy receipt opisuje
                             TĘ treść. Bez tego weryfikujesz podpis, ale nie to,
                             czy dotyczy tekstu, który trzymasz w ręku.
    --tokens <plik>          [z --verify-receipt] M1: plik JSON
                             {tokenizer_hash, client_nonce, prompt_token_ids[],
                             output_token_ids[]} — sprawdź wiązanie wejścia/stanu.
    --audyt <plik>           [z --verify-receipt] M3: plik JSON
                             {kroki:[{indeks,node_token,topk:[[id,lp],..]}]} —
                             ocena containment top-k (polityka domyślna k=2).
    --verify-receipt <plik>  sprawdź receipt, który ktoś Ci podał (`-` = stdin).
                             Offline, bez sieci. Z --expect-job-id/--expect-model
                             sprawdza też, czy to receipt do TEGO zlecenia.

PODKOMENDA (alias na --verify-receipt):
    simon verify <receipt> [--output <plik>] [--job-id <id>] [--model <m>] [--json]
                             to samo, czytelniej. `--output` = `--expect-output`.
    --key-file <plik>        [node] jak --key, ale ziarno czytane z pliku.
                             UŻYWAJ TEGO we wdrożeniu: --key <hex> jest widoczny
                             w `ps` dla każdego użytkownika maszyny.
    --key <hex>              [node] trwały klucz Ed25519 (64 hex = 32 B seed).
                             Bez tego node losuje klucz przy starcie i klient
                             nie ma stałej tożsamości do weryfikacji receiptu.
    -h, --help               ta pomoc

PRZYKŁADY:
    # node na Szponie (vLLM na 18020)
    simon --role node --listen /ip4/0.0.0.0/tcp/9001 --model-url http://127.0.0.1:18020 --model-hash qwen3.8-27b

    # node na Franku, podpięty do Szpona
    simon --role node --listen /ip4/0.0.0.0/tcp/9002 --bootstrap /ip4/<IP_SZPONA>/tcp/9001 --model-url http://127.0.0.1:8000 --model-hash bielik-awq

    # agent (klient) — łączy się z nodem
    simon --role agent --bootstrap /ip4/<IP_NODA>/tcp/9001 --prompt "policz 2+2"

    # agent, duży plik (M3.x map-reduce): pytanie stosowane do każdego kawałka
    simon --role agent --bootstrap /ip4/<IP_NODA>/tcp/9001 \
        --file duzy_dokument.txt --prompt "streść kluczowe fakty" \
        --chunk-tokens 4000 --map-max-tokens 200

    # agent, łańcuch B2: CODER (node 1) -> TESTER (node 2), bez ręcznego przeklejania
    simon --role agent --bootstrap /ip4/<IP_CODER>/tcp/9001 \
        --prompt "napisz funkcję sumującą dwie liczby w Rust" \
        --then-bootstrap /ip4/<IP_TESTER>/tcp/9002 \
        --then-prompt "znajdź przypadek brzegowy dla tej funkcji"

    # weryfikacja cudzego receiptu offline (ta sama komenda co PODKOMENDA)
    simon verify przyklady/receipt.json --output przyklady/odpowiedz.txt

UWAGA (RULES #1): prompt idzie kanałem PUNKT-PUNKT (/simon/prompt/1),
nigdy przez gossipsub. Gossipsubem idzie tylko zlecenie bez treści.
"#;

/// Rola procesu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rola {
    Agent,
    Coord,
    Node,
}

/// Sparsowane argumenty.
#[derive(Debug, Clone)]
pub struct Opcje {
    pub rola: Rola,
    pub listen: Option<String>,
    pub bootstrap: Vec<String>,
    pub model_url: Option<String>,
    pub model_hash: Option<String>,
    pub prompt: Option<String>,
    pub max_tokens: u32,
    pub fee: u64,
    /// M2.5.1: trwały klucz node'a (hex, 32 B seed). Bez tego node losuje
    /// klucz przy każdym starcie i klient nie ma stałej tożsamości do weryfikacji.
    pub key: Option<String>,
    /// M3.x: plik większy niż limit node'a — włącza chunkowanie + map-reduce.
    pub file: Option<String>,
    pub chunk_tokens: u32,
    pub map_max_tokens: u32,
    /// B2: łańcuch agent→agent (przekazanie zweryfikowanego wyniku etapu A
    /// jako danych wejściowych etapu B, do innego node'a/modelu).
    pub then_bootstrap: Vec<String>,
    pub then_model_hash: Option<String>,
    pub then_prompt: Option<String>,
    /// Demo/integracja: jeden obiekt JSON na stdout zamiast tekstu dla czlowieka.
    pub json: bool,
    /// Sciezka do pliku z kluczem node'a. Wersja `--key <hex>` wystawia ziarno
    /// w `ps` KAZDEMU uzytkownikowi maszyny (i w dzienniku systemd) — do
    /// trwalego wdrozenia uzywaj pliku.
    pub key_file: Option<String>,
    /// Weryfikacja cudzego receiptu — bez sieci, bez zaufania do kogokolwiek.
    /// To jest cala teza projektu jako jedna komenda: dostales wynik i podpis,
    /// sprawdzasz je sam. Wartosc: sciezka do pliku albo `-` (stdin).
    pub verify_receipt: Option<String>,
    pub expect_job_id: Option<String>,
    pub expect_model: Option<String>,
    /// Plik z odpowiedzia, ktora rzekomo opisuje receipt (`-` = stdin nie dziala,
    /// bo stdin czyta receipt). Bez tego sprawdzasz podpis, ale NIE to, czy
    /// dotyczy tekstu, ktory trzymasz.
    pub expect_output: Option<String>,
    /// M5.2: plik lokalnego metrycznika pracy (JSON Lines, tylko dopisywanie).
    /// NIE jest to portfel ani saldo — zapis, co się wydarzyło i do jakiego
    /// poziomu zostało sprawdzone. Wykrywa też powtórzenie receiptu.
    pub rejestr: Option<String>,
    /// M5.2a: plik trwałej tożsamości klienta. Bez tego domyślny katalog danych
    /// użytkownika. NIGDY nie przyjmujemy sekretu jako argumentu ani zmiennej
    /// środowiskowej — byłby widoczny w `ps` i w historii powłoki.
    pub identity_file: Option<String>,
    /// M1: plik JSON z wiązaniem wejścia/stanu do sprawdzenia w receipcie:
    /// `{tokenizer_hash, client_nonce, prompt_token_ids[], output_token_ids[]}`.
    pub tokens: Option<String>,
    /// M3: plik JSON z audytem wykonania: `{"kroki":[{indeks,node_token,topk:[[id,lp]..]}]}`.
    /// Oceniany polityką containment (top-k + margines).
    pub audyt: Option<String>,
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
        let mut rejestr = None;
        let mut identity_file = None;
        let mut tokens = None;
        let mut audyt = None;

        let mut i = 0;
        while i < args.len() {
            let arg = args[i].as_str();
            let wartosc = |i: &mut usize, nazwa: &str| -> Result<String, String> {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .ok_or_else(|| format!("brak wartości dla {nazwa}"))
            };
            match arg {
                "-h" | "--help" => return Err(USAGE.to_string()),
                "--role" => {
                    let v = wartosc(&mut i, "--role")?;
                    rola = Some(match v.as_str() {
                        "agent" => Rola::Agent,
                        "coord" => Rola::Coord,
                        "node" => Rola::Node,
                        inny => return Err(format!("nieznana rola: {inny}")),
                    });
                }
                "--listen" => listen = Some(wartosc(&mut i, "--listen")?),
                "--bootstrap" => bootstrap.push(wartosc(&mut i, "--bootstrap")?),
                "--model-url" => model_url = Some(wartosc(&mut i, "--model-url")?),
                "--model-hash" => model_hash = Some(wartosc(&mut i, "--model-hash")?),
                "--prompt" => prompt = Some(wartosc(&mut i, "--prompt")?),
                "--max-tokens" => {
                    let v = wartosc(&mut i, "--max-tokens")?;
                    max_tokens = v.parse().map_err(|_| format!("zły --max-tokens: {v}"))?;
                }
                "--fee" => {
                    let v = wartosc(&mut i, "--fee")?;
                    fee = v.parse().map_err(|_| format!("zły --fee: {v}"))?;
                }
                "--key" => key = Some(wartosc(&mut i, "--key")?),
                "--file" => file = Some(wartosc(&mut i, "--file")?),
                "--chunk-tokens" => {
                    let v = wartosc(&mut i, "--chunk-tokens")?;
                    chunk_tokens = v.parse().map_err(|_| format!("zły --chunk-tokens: {v}"))?;
                    if chunk_tokens == 0 {
                        return Err("--chunk-tokens musi być > 0".to_string());
                    }
                }
                "--map-max-tokens" => {
                    let v = wartosc(&mut i, "--map-max-tokens")?;
                    map_max_tokens = v.parse().map_err(|_| format!("zły --map-max-tokens: {v}"))?;
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
                "--rejestr" => rejestr = Some(wartosc(&mut i, "--rejestr")?),
                "--identity-file" => identity_file = Some(wartosc(&mut i, "--identity-file")?),
                "--tokens" => tokens = Some(wartosc(&mut i, "--tokens")?),
                "--audyt" => audyt = Some(wartosc(&mut i, "--audyt")?),
                inny => return Err(format!("nieznany argument: {inny}")),
            }
            i += 1;
        }

        let rola = match rola {
            Some(r) => r,
            // --verify-receipt dziala offline, rola jest bez znaczenia
            None if verify_receipt.is_some() => Rola::Agent,
            None => return Err("brak --role (agent|coord|node)".into()),
        };
        Ok(Self {
            rola,
            listen,
            bootstrap,
            model_url,
            model_hash,
            prompt,
            max_tokens,
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
            rejestr,
            identity_file,
            tokens,
            audyt,
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

/// `--verify-receipt`: sprawdza receipt, ktory ktos nam podal. Bez sieci,
/// bez runtime, bez zaufania do zrodla. Kazda bramka raportowana OSOBNO —
/// "nie przeszlo" bez wskazania KTOREJ bramki jest bezuzyteczne dla kogos,
/// kto probuje zrozumiec, co go oszukalo.
fn weryfikuj_offline(opcje: &Opcje, zrodlo: &str) -> ExitCode {
    use std::io::Read;
    let tekst = if zrodlo == "-" {
        let mut b = String::new();
        match std::io::stdin().read_to_string(&mut b) {
            Ok(_) => b,
            Err(e) => {
                eprintln!("BŁĄD: nie mogę czytać stdin: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        match std::fs::read_to_string(zrodlo) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("BŁĄD: nie mogę czytać {zrodlo}: {e}");
                return ExitCode::FAILURE;
            }
        }
    };

    let r: simon_core::receipt::Receipt = match serde_json::from_str(tekst.trim()) {
        Ok(r) => r,
        Err(e) => {
            if opcje.json {
                println!("{}", serde_json::json!({
                    "ok": false, "parsuje_sie": false, "powod": e.to_string()}));
            } else {
                eprintln!("NIEPOPRAWNY: to nie jest receipt ({e})");
            }
            return ExitCode::FAILURE;
        }
    };

    let podpis_ok = r.verify_self().is_ok();
    // Czy receipt opisuje TEN tekst — bramka, ktorej brakowalo do 2026-09-18.
    let tresc_ok = match opcje.expect_output.as_deref() {
        Some(sciezka) => match std::fs::read_to_string(sciezka) {
            Ok(t) => Some(r.zgodny_z_wyjsciem(&t)),
            Err(e) => {
                eprintln!("BŁĄD: nie mogę czytać --expect-output {sciezka}: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let job_ok = opcje.expect_job_id.as_ref().map(|j| *j == r.job_id);
    let model_ok = opcje.expect_model.as_ref().map(|m| *m == r.model_hash);

    // M1: opcjonalne wiązanie wejścia/stanu (tokeny po stronie klienta).
    let m1_wynik = match opcje.tokens.as_deref() {
        Some(sciezka) => {
            let tekst = match std::fs::read_to_string(sciezka) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("BŁĄD: nie mogę czytać --tokens {sciezka}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            match serde_json::from_str::<PlikTokenow>(&tekst) {
                Ok(p) => Some(r.zweryfikuj_m1(
                    &p.tokenizer_hash,
                    &p.client_nonce,
                    &p.prompt_token_ids,
                    &p.output_token_ids,
                )),
                Err(e) => {
                    eprintln!("BŁĄD: zły format --tokens {sciezka}: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        None => None,
    };
    let m1_ok = m1_wynik.as_ref().map(|w| w.is_ok());

    // M3: opcjonalny audyt containment (top-k + margines).
    let m3_wynik = match opcje.audyt.as_deref() {
        Some(sciezka) => {
            let tekst = match std::fs::read_to_string(sciezka) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("BŁĄD: nie mogę czytać --audyt {sciezka}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            match simon_core::m3::ocena_json(&tekst, &simon_core::m3::Polityka::default()) {
                Ok(w) => Some(w),
                Err(e) => {
                    eprintln!("BŁĄD: zły format --audyt {sciezka}: {e}");
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
        && tresc_ok != Some(false)
        && m1_ok != Some(false)
        && m3_ok != Some(false);

    if opcje.json {
        println!("{}", serde_json::json!({
            "ok": ok,
            "parsuje_sie": true,
            "podpis_ok": podpis_ok,
            "job_id_ok": job_ok,
            "model_ok": model_ok,
            "tresc_ok": tresc_ok,
            "poziom": r.poziom(),
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
        println!("podpis Ed25519 : {}", if podpis_ok { "OK" } else { "ZŁY" });
        if let Some(v) = job_ok { println!("job_id         : {}", if v { "zgodny" } else { "NIEZGODNY" }); }
        if let Some(v) = model_ok { println!("model_hash     : {}", if v { "zgodny" } else { "NIEZGODNY" }); }
        if let Some(v) = tresc_ok { println!("treść wyniku   : {}", if v { "zgodna z odciskiem" } else { "NIEZGODNA — to nie jest ten wynik" }); }
        if let Some(w) = m1_wynik.as_ref() {
            match w {
                Ok(()) => println!("wiązanie M1    : OK (tokenizer + nonce + prompt + wyjście)"),
                Err(e) => println!("wiązanie M1    : ZŁE — {e}"),
            }
        }
        if let Some(w) = m3_wynik.as_ref() {
            println!(
                "audyt M3       : {} (kroków={}, w top-k={}, poza={}, najgorszy rank={}, margines={:.4})",
                if w.ok { "OK" } else { "ODRZUCONY" },
                w.krokow, w.w_topk, w.poza_topk, w.najgorszy_rank, w.najgorszy_margin
            );
            if let Some(p) = &w.powod {
                println!("                 {p}");
            }
        }
        let pz = r.poziom();
        println!("poziom dowodu  : {pz}");
        if pz == 0 {
            println!(
                "UWAGA          : poziom 0 = tylko autorstwo/podpis; BRAK wiązania wejścia/stanu (M1).\n                 \
                 Nie przedstawiać jako weryfikacji wykonania (docs/THREAT-MODEL.md)."
            );
        }
        println!("node           : {}", r.node_id);
        println!("werdykt        : {}", if ok { "RECEIPT WAŻNY" } else { "ODRZUCONY" });
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// `simon verify <receipt> [--output f] [--job-id id] [--model m] [--json]`
/// rozwija się do flag `--verify-receipt` / `--expect-*` — ta sama ścieżka, ten
/// sam kod (`weryfikuj_offline`), zero drugiej implementacji. Każdy inny
/// argument (w tym nieznany) idzie jak jest; parser go zgłosi.
fn rozwin_podkomende(args: Vec<String>) -> Vec<String> {
    if args.first().map(String::as_str) != Some("verify") {
        return args;
    }
    let mut out: Vec<String> = Vec::new();
    let mut receipt: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        let a = args[i].clone();
        // Flaga z wartością: `cel` to nazwa po przejściu, `args[i+1]` to wartość.
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
            "--json" => out.push("--json".into()),
            "-h" | "--help" => out.push("--help".into()),
            _ if a == "-" || !a.starts_with('-') => {
                if receipt.is_none() {
                    receipt = Some(a);
                } else {
                    out.push(a); // nadmiarowy pozycyjny — parser zgłosi
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
        // `simon verify` bez pliku i bez --help: niech parser powie, czego brak.
        None if !out.iter().any(|s| s == "--help") => {
            final_args.push("--verify-receipt".into());
        }
        None => {}
    }
    final_args.extend(out);
    final_args
}

/// `simon audyt <plik.json> [--poziom N]` — ocenia sam audyt M3 (containment top-k),
/// bez receiptu. Dla verifier-loop: inferencję robi strona zewnętrzna (llama.cpp),
/// a decyzję (Pass/SoftFail/HardFail) liczy Rust. `--poziom` = poziom receiptu (dom. 1).
fn audyt_podkomenda(args: &[String]) -> ExitCode {
    let mut plik: Option<&str> = None;
    let mut poziom: u8 = 1;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--poziom" => {
                i += 1;
                poziom = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(1);
            }
            "-h" | "--help" => {
                println!(
                    "simon audyt <plik.json> [--poziom N]\n  \
                     ocenia audyt M3 (kroki: node_token vs top-k) polityką k=2/margin 0,25;\n  \
                     decyzja: Pass / SoftFail (bez slasha) / HardFail / BrakPodstaw"
                );
                return ExitCode::SUCCESS;
            }
            inny => plik = Some(inny),
        }
        i += 1;
    }
    let Some(sciezka) = plik else {
        eprintln!("BŁĄD: podaj plik audytu: simon audyt <plik.json>");
        return ExitCode::FAILURE;
    };
    let tekst = match std::fs::read_to_string(sciezka) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("BŁĄD: nie mogę czytać {sciezka}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match simon_core::m3::ocena_json(&tekst, &simon_core::m3::Polityka::default()) {
        Ok(w) => {
            let d = simon_core::m3::decyzja(&w, poziom, &simon_core::m3::Progi::default());
            println!(
                "audyt      : kroków={} w top-k={} poza-top-k={} najgorszy rank={} margines={:.4}",
                w.krokow, w.w_topk, w.poza_topk, w.najgorszy_rank, w.najgorszy_margin
            );
            println!("werdykt M3 : {}", if w.ok { "OK" } else { "NIE" });
            println!("decyzja    : {d:?} (poziom receiptu {poziom})");
            if let Some(p) = &w.powod {
                println!("powód      : {p}");
            }
            if matches!(d, simon_core::m3::Decyzja::HardFail) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("BŁĄD: zły audyt: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `simon guard [--url U] [--stdin] "tekst"` — brama wejścia (guard CPU na loopbacku).
/// POST `{text}` → `{ryzyko, pii, sekret, kategorie, ms_*}`. Wyjście != 0 gdy `ryzyko`.
/// To jest wpięcie guarda w harness po stronie klienta (pre-send); ten sam wzorzec
/// działa dla node'a (pre-compute). UWAGA: guard widzi treść (RULES #1) i NIE łapie
/// prompt injection — patrz `docs/THREAT-MODEL.md`.
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
                println!("simon guard [--url http://127.0.0.1:19301/guard] [--stdin] \"tekst\"");
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
            eprintln!("BŁĄD: nie mogę czytać stdin");
            return ExitCode::FAILURE;
        }
        tekst = Some(s);
    }
    let Some(t) = tekst else {
        eprintln!("BŁĄD: podaj tekst albo --stdin");
        return ExitCode::FAILURE;
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("BŁĄD: runtime: {e}");
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
                eprintln!("BŁĄD: guard niedostępny pod {url} ({e})");
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
            // --help to nie błąd, tylko pomoc.
            if msg.starts_with('\n') {
                println!("{msg}");
                return ExitCode::SUCCESS;
            }
            eprintln!("BŁĄD: {msg}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    // Weryfikacja cudzego receiptu jest offline — nie stawiamy runtime'u
    // ani nie dotykamy sieci.
    if let Some(zrodlo) = opcje.verify_receipt.clone() {
        return weryfikuj_offline(&opcje, &zrodlo);
    }

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("BŁĄD: nie mogę wystartować runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    let wynik = rt.block_on(async {
        match opcje.rola {
            Rola::Node => node::uruchom(&opcje).await,
            Rola::Agent => agent::uruchom(&opcje).await,
            Rola::Coord => coord::uruchom(&opcje).await,
        }
    });

    match wynik {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("BŁĄD: {e}");
            ExitCode::FAILURE
        }
    }
}
