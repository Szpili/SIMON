#!/usr/bin/env bash
# Detect what the AMD image already provides. Do not install anything here.
set -euo pipefail

echo "=== ROCm ==="
ROCM="${ROCM_PATH:-/opt/rocm}"
if [ -d "$ROCM" ]; then
  echo "ROCM_PATH=$ROCM"
  [ -x "$ROCM/bin/hipcc" ] && "$ROCM/bin/hipcc" --version 2>&1 | head -1 || echo "  (no hipcc)"
  command -v rocminfo >/dev/null && rocminfo 2>/dev/null | grep -m1 'Name:.*gfx' || true
else
  echo "no ROCm at $ROCM (set ROCM_PATH)"
fi

echo "=== vLLM (image may preinstall it) ==="
python3 -c 'import vllm, sys; print("vllm", vllm.__version__)' 2>/dev/null || echo "  no vllm module"
command -v vllm >/dev/null && echo "  vllm CLI: $(command -v vllm)" || true

echo "=== llama.cpp HIP build ==="
LLAMA="${LLAMA_CPP:-$HOME/llama.cpp}"
if [ -x "$LLAMA/build-hip/bin/llama-server" ]; then
  echo "  present: $LLAMA/build-hip/bin/llama-server"
else
  echo "  absent -> build_hip.sh will build it ($LLAMA/build-hip)"
fi

echo "=== inputs ==="
echo "  SIMON_REF_URL=${SIMON_REF_URL:-<unset>}"
echo "  SIMON_GGUF=${SIMON_GGUF:-<unset>}"
[ -n "${SIMON_GGUF:-}" ] && [ -f "${SIMON_GGUF}" ] && echo "  gguf: $(ls -la "$SIMON_GGUF")" || echo "  gguf: missing"

echo "=== reachability of the NVIDIA reference ==="
if [ -n "${SIMON_REF_URL:-}" ]; then
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 8 "${SIMON_REF_URL%/}/health" || true)
  echo "  GET ${SIMON_REF_URL}/health -> ${code:-fail}"
else
  echo "  set SIMON_REF_URL to test"
fi

echo "=== tools ==="
for t in cmake curl python3 ffmpeg; do printf '  %-8s %s\n' "$t" "$(command -v $t || echo MISSING)"; done
