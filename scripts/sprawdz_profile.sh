#!/usr/bin/env bash
# Invariant: two DIFFERENT execution profiles cannot yield identical throughput.
#
# This test catches incidents where generation time was calculated from a constant
# throughput (e.g. 65 tok/s) and both nodes reported identical numbers despite different GPUs.
# Requires live nodes.
#
# Usage: scripts/sprawdz_profile.sh [nodes_list.json]
set -euo pipefail

SIMON="${SIMON_BIN:-$(dirname "$0")/../target/release/simon}"
NODES="${1:-$HOME/.simon/nodes.json}"
PROMPT="Write two sentences about verifiable computation in distributed networks."

[ -f "$NODES" ] || { echo "missing nodes list: $NODES" >&2; exit 2; }

python3 - "$SIMON" "$NODES" "$PROMPT" <<'PY'
import json, subprocess, sys

simon, plik, prompt = sys.argv[1], sys.argv[2], sys.argv[3]
wezly = json.load(open(plik))
if len(wezly) < 2:
    print("at least two nodes with different profiles required", file=sys.stderr)
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
        print(f"NODE DID NOT RESPOND: {w['nazwa']}", file=sys.stderr)
        raise SystemExit(2)

    obs = d.get("client_observation", d.get("obserwacja_klienta"))
    # Throughput calculated from time MEASURED BY CLIENT, not from node declaration.
    tps = d["tokens_out"] / (obs["observed_time_to_complete_ms"] / 1000.0)
    wyniki.append({
        "nazwa": w["nazwa"], "runtime": d["receipt"]["runtime"],
        "tokens": d["tokens_out"], "zmierzony_ms": obs["observed_time_to_complete_ms"],
        "deklarowany_gen_ms": d["node_declared_gen_ms"], "tps": tps,
    })
    print(f"  {w['nazwa']:38} {d['receipt']['runtime']:16} "
          f"{tps:7.2f} tok/s  (measured {obs['observed_time_to_complete_ms']} ms, "
          f"declared {d['node_declared_gen_ms']} ms)")

blad = 0
for i in range(len(wyniki)):
    for j in range(i + 1, len(wyniki)):
        a, b = wyniki[i], wyniki[j]
        skala = max(abs(a["tps"]), abs(b["tps"]))
        if skala and abs(a["tps"] - b["tps"]) / skala < 0.001:
            print(f"\nERROR: {a['nazwa']} and {b['nazwa']} produce IDENTICAL throughput "
                  f"({a['tps']:.3f}). This is a signature of a constant posing as a measurement.", file=sys.stderr)
            blad = 1

# Node declaration must not diverge from client-measured time by more than transport overhead.
for w in wyniki:
    roznica = w["zmierzony_ms"] - w["deklarowany_gen_ms"]
    if roznica < 0:
        print(f"\nWARNING: {w['nazwa']} declares LONGER time than measured at client "
              f"({w['deklarowany_gen_ms']} > {w['zmierzony_ms']} ms) - impossible without clock issue.", file=sys.stderr)
        blad = 1

raise SystemExit(blad)
PY
