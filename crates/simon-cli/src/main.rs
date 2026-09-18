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
    --expect-output <plik>   [z --verify-receipt] sprawdź, czy receipt opisuje
                             TĘ treść. Bez tego weryfikujesz podpis, ale nie to,
                             czy dotyczy tekstu, który trzymasz w ręku.
    --verify-receipt <plik>  sprawdź receipt, który ktoś Ci podał (`-` = stdin).
                             Offline, bez sieci. Z --expect-job-id/--expect-model
                             sprawdza też, czy to receipt do TEGO zlecenia.
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
        })
    }
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
    let ok = podpis_ok
        && job_ok != Some(false)
        && model_ok != Some(false)
        && tresc_ok != Some(false);

    if opcje.json {
        println!("{}", serde_json::json!({
            "ok": ok,
            "parsuje_sie": true,
            "podpis_ok": podpis_ok,
            "job_id_ok": job_ok,
            "model_ok": model_ok,
            "tresc_ok": tresc_ok,
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
        println!("node           : {}", r.node_id);
        println!("werdykt        : {}", if ok { "RECEIPT WAŻNY" } else { "ODRZUCONY" });
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
