#!/usr/bin/env bash
# Run the SIMON node on AMD, backed by the local HIP llama-server (hackathon: AMD in product).
set -euo pipefail
: "${SIMON_MODEL_HASH:?export SIMON_MODEL_HASH (e.g. qwen2.5-3b-instruct-q8)}"
PORT="${AMD_PORT:-18301}"
SIMON_REPO="${SIMON_REPO:-$HOME/SIMON-git}"

cd "$SIMON_REPO"
echo "== building simon-cli =="
cargo build --release -p simon-cli

echo "== node on AMD -> model at http://127.0.0.1:$PORT =="
exec ./target/release/simon --role node \
  --listen /ip4/0.0.0.0/tcp/9001 \
  --model-url "http://127.0.0.1:$PORT" \
  --model-hash "$SIMON_MODEL_HASH"
