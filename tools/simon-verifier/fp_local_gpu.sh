#!/usr/bin/env bash
# Opportunistic local GPU arms (only two still matter, per the pre-registration:
# kv_q8 -> may justify "KV f16 or q8" as a declared execution profile; Vulkan -> closest
# local cross-vendor stand-in). One command, run when the 3090 is free. Never stops
# anything (strata/MiMo/others); it exits early if VRAM is short.
#
#   bash tools/simon-verifier/fp_local_gpu.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
LLAMA="${LLAMA_CPP:-$HOME/llama.cpp}"
MODEL="${SIMON_GGUF:-$HOME/.cache/qwen-sub/3b/Qwen2.5-3B-Instruct-Q8_0.gguf}"
CUDA_BIN="$LLAMA/build-cuda129/bin/llama-server"
OUT="${OUT:-$HOME/reports/simon-fp-local-gpu}"
PROMPTS="$HERE/prompts_fp.txt"
NEED_GB="${NEED_GB:-9}"          # ref (~3.5) + one config (~3.5) + headroom
GPU_INDEX="${SIMON_GPU:-1}"      # 1 = RTX 3090 (0 = GTX Titan X on this box)
REF_PORT=18201; KVQ8_PORT=18207; VK_PORT=18208
export CUDA_VISIBLE_DEVICES="$GPU_INDEX"   # CUDA build is sm_86 -> 3090 only
mkdir -p "$OUT"

[ -f "$MODEL" ] || { echo "no model at $MODEL (set SIMON_GGUF)"; exit 1; }
[ -x "$CUDA_BIN" ] || { echo "no CUDA llama-server at $CUDA_BIN"; exit 1; }

# NB: no pipe -> avoids SIGPIPE+pipefail; query the 3090 explicitly, not device 0.
free_mb=$(nvidia-smi -i "$GPU_INDEX" --query-gpu=memory.free --format=csv,noheader,nounits 2>/dev/null)
free_mb="${free_mb//[^0-9]/}"; free_mb="${free_mb:-0}"
echo "free VRAM on GPU $GPU_INDEX: ${free_mb} MiB; need ~${NEED_GB} GB"
if [ -z "${free_mb:-}" ] || [ "$free_mb" -lt $((NEED_GB*1024)) ]; then
  echo "3090 too full (strata or another service). Not stopping anything — re-run when free."
  echo "hint: systemctl --user stop strata-serve.service   # only with the user's OK"
  exit 3
fi

PIDS=()
cleanup(){ for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT
wait_ok(){ for _ in $(seq 1 80); do curl -s --max-time 3 "http://127.0.0.1:$1/health" 2>/dev/null | grep -q '"ok"' && return 0; sleep 3; done; return 1; }

echo "== reference (CUDA, KV f16) on :$REF_PORT =="
setsid "$CUDA_BIN" -m "$MODEL" -ngl 99 -c 1024 --port "$REF_PORT" >"$OUT/ref.log" 2>&1 </dev/null &
PIDS+=($!); wait_ok "$REF_PORT" || { echo "ref failed"; tail -20 "$OUT/ref.log"; exit 1; }

echo "== kv_q8 (CUDA, KV q8_0) on :$KVQ8_PORT =="
setsid "$CUDA_BIN" -m "$MODEL" -ngl 99 -c 1024 --cache-type-k q8_0 --cache-type-v q8_0 --port "$KVQ8_PORT" >"$OUT/kv_q8.log" 2>&1 </dev/null &
PIDS+=($!); wait_ok "$KVQ8_PORT" || { echo "kv_q8 failed"; tail -20 "$OUT/kv_q8.log"; exit 1; }

CFGS=(--cfg "kv_q8=http://127.0.0.1:$KVQ8_PORT")
if [ -x "$LLAMA/build-vulkan/bin/llama-server" ]; then
  echo "== vulkan on :$VK_PORT =="
  setsid "$LLAMA/build-vulkan/bin/llama-server" -m "$MODEL" -ngl 99 -c 1024 --port "$VK_PORT" >"$OUT/vulkan.log" 2>&1 </dev/null &
  PIDS+=($!)
  if wait_ok "$VK_PORT"; then CFGS+=(--cfg "vulkan=http://127.0.0.1:$VK_PORT"); else echo "vulkan failed (see log), continuing without it"; fi
else
  echo "(no build-vulkan; skipping. Build: cmake -DGGML_VULKAN=ON)"
fi

echo "== FP matrix vs reference =="
python3 "$HERE/fp_matrix.py" --ref "http://127.0.0.1:$REF_PORT" "${CFGS[@]}" \
  --prompts "$PROMPTS" --tokens 20 --out "$OUT/local_gpu_fp.jsonl" | tee "$OUT/local_gpu_fp.txt"
echo "== collected: $OUT/local_gpu_fp.jsonl =="
