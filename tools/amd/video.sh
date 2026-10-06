#!/usr/bin/env bash
# Record the AMD session. Ctrl-C to stop.
set -euo pipefail
OUT="${AMD_OUT:-$HOME/amd-fp}"
mkdir -p "$OUT"

if command -v ffmpeg >/dev/null && [ -n "${DISPLAY:-}" ]; then
  echo "== recording screen to $OUT/session.mp4 (Ctrl-C to stop) =="
  exec ffmpeg -y -f x11grab -framerate 30 -i "${DISPLAY}.0" \
    -c:v libx264 -pix_fmt yuv420p "$OUT/session.mp4"
fi

echo "no ffmpeg or no DISPLAY — record manually. Suggested shots (in order):"
cat <<'EOF'
1. env.sh output: ROCm detected, HIP llama-server built
2. HIP /health OK, model loaded
3. fp_matrix.py verdict on AMD (expected FAIL, as pre-registered)
4. SIMON node on AMD: --listen /ip4/0.0.0.0/tcp/9001, model-hash shown
5. dashboard / offline verifier checking a receipt on the AMD box
EOF
