#!/usr/bin/env bash
# Inwariant: dwa RÓŻNE profile wykonania nie mogą dać tej samej przepustowości.
#
# Ten test złapałby incydent z 2026-09-18, gdy `gen_ms` liczono ze stałej
# 65 tok/s i oba węzły raportowały identyczną liczbę mimo różnych kart.
# Wymaga żywych węzłów — dlatego skrypt, nie `cargo test`.
#
# Użycie: scripts/sprawdz_profile.sh [plik-z-lista-wezlow.json]
set -euo pipefail

SIMON="${SIMON_BIN:-$(dirname "$0")/../target/release/simon}"
WEZLY="${1:-$HOME/.simon/nodes.json}"
PROMPT="Napisz dwa zdania o weryfikacji obliczeń w sieci rozproszonej."

[ -f "$WEZLY" ] || { echo "brak listy węzłów: $WEZLY" >&2; exit 2; }

python3 - "$SIMON" "$WEZLY" "$PROMPT" <<'PY'
import json, subprocess, sys

simon, plik, prompt = sys.argv[1], sys.argv[2], sys.argv[3]
wezly = json.load(open(plik))
if len(wezly) < 2:
    print("potrzebne co najmniej dwa węzły o różnych profilach", file=sys.stderr)
    raise SystemExit(2)

wyniki = []
for w in wezly:
    p = subprocess.run(
        [simon, "--role", "agent", "--bootstrap", w["adres"], "--model-hash", w["model"],
         "--prompt", prompt, "--max-tokens", "40", "--json"],
        capture_output=True, text=True, timeout=400)
    d = None
    for linia in reversed(p.stdout.strip().splitlines()):
        try:
            d = json.loads(linia); break
        except json.JSONDecodeError:
            continue
    if not d or not d.get("ok"):
        print(f"WĘZEŁ NIE ODPOWIEDZIAŁ: {w['nazwa']}", file=sys.stderr)
        raise SystemExit(2)

    obs = d["obserwacja_klienta"]
    # Przepustowość liczymy z czasu ZMIERZONEGO PRZEZ KLIENTA, nie z deklaracji.
    tps = d["tokens_out"] / (obs["observed_time_to_complete_ms"] / 1000.0)
    wyniki.append({
        "nazwa": w["nazwa"], "runtime": d["receipt"]["runtime"],
        "tokens": d["tokens_out"], "zmierzony_ms": obs["observed_time_to_complete_ms"],
        "deklarowany_gen_ms": d["node_declared_gen_ms"], "tps": tps,
    })
    print(f"  {w['nazwa']:38} {d['receipt']['runtime']:16} "
          f"{tps:7.2f} tok/s  (zmierzone {obs['observed_time_to_complete_ms']} ms, "
          f"deklarowane {d['node_declared_gen_ms']} ms)")

blad = 0
for i in range(len(wyniki)):
    for j in range(i + 1, len(wyniki)):
        a, b = wyniki[i], wyniki[j]
        skala = max(abs(a["tps"]), abs(b["tps"]))
        if skala and abs(a["tps"] - b["tps"]) / skala < 0.001:
            print(f"\nBŁĄD: {a['nazwa']} i {b['nazwa']} dają IDENTYCZNĄ przepustowość "
                  f"({a['tps']:.3f}). To sygnatura stałej udającej pomiar.", file=sys.stderr)
            blad = 1

# Deklaracja node'a nie może się rozjeżdżać z czasem mierzonym u klienta
# bardziej niż o narzut transportu; duża rozbieżność to sygnał, nie dowód.
for w in wyniki:
    roznica = w["zmierzony_ms"] - w["deklarowany_gen_ms"]
    if roznica < 0:
        print(f"\nUWAGA: {w['nazwa']} deklaruje DŁUŻSZY czas niż zmierzony u klienta "
              f"({w['deklarowany_gen_ms']} > {w['zmierzony_ms']} ms) — niemożliwe bez kłamstwa "
              f"albo błędu zegara.", file=sys.stderr)
        blad = 1

raise SystemExit(blad)
PY
