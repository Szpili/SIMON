#!/usr/bin/env bash
# FP arm on AMD: same GGUF, same prompts, same fp_matrix.py. Reference = NVIDIA over tailnet.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
VERIF="$(cd "$HERE/../simon-verifier" && pwd)"

: "${SIMON_REF_URL:?export SIMON_REF_URL (NVIDIA reference, e.g. http://<szpon-tailscale>:18201)}"
: "${SIMON_GGUF:?export SIMON_GGUF (the same Qwen2.5-*-Instruct-Q8_0.gguf)}"
PORT="${AMD_PORT:-18301}"
OUT="${AMD_OUT:-$HOME/amd-fp}"
LLAMA="${LLAMA_CPP:-$HOME/llama.cpp}"
BIN="$LLAMA/build-hip/bin/llama-server"
mkdir -p "$OUT"

[ -x "$BIN" ] || { echo "no HIP llama-server at $BIN (run build_hip.sh)"; exit 1; }

# stop a previous instance on this port
p=$(ss -tlnp 2>/dev/null | awk -v P=":$PORT " '$0 ~ P' | grep -oE 'pid=[0-9]+' | head -1 | cut -d= -f2)
[ -n "${p:-}" ] && kill "$p" 2>/dev/null || true
sleep 1

echo "== starting HIP llama-server on :$PORT =="
setsid "$BIN" -m "$SIMON_GGUF" -ngl 99 -c 1024 --port "$PORT" >"$OUT/server.log" 2>&1 </dev/null &
for i in $(seq 1 80); do curl -s --max-time 3 "http://127.0.0.1:$PORT/health" 2>/dev/null | grep -q '"ok"' && { echo "ready (${i}x3s)"; break; }; sleep 3; done
curl -s --max-time 3 "http://127.0.0.1:$PORT/health" | grep -q '"ok"' || { echo "server failed to start; see $OUT/server.log"; tail -20 "$OUT/server.log"; exit 1; }

echo "== FP matrix: node=amd(:$PORT)  reference=$SIMON_REF_URL =="
python3 "$VERIF/fp_matrix.py" \
  --ref "$SIMON_REF_URL" \
  --cfg "amd=http://127.0.0.1:$PORT" \
  --prompts "$VERIF/prompts_fp.txt" \
  --tokens 20 --out "$OUT/amd_fp.jsonl" | tee "$OUT/amd_fp.txt"

echo "== collected: $OUT/amd_fp.jsonl =="
