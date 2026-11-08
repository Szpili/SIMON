#!/usr/bin/env bash
# Builds receipt verifier from Rust (simon-core) to WASM and generates JS glue.
# Single source of truth — UI does not reimplement crypto.
#
# Requires: rust + target wasm32-unknown-unknown + wasm-bindgen CLI:
#   rustup target add wasm32-unknown-unknown
#   cargo install -f wasm-bindgen-cli --version 0.2.108
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/../.." # repo root

cargo build --release --target wasm32-unknown-unknown -p simon-verify-wasm

wasm-bindgen \
  --target web \
  --out-dir ui/src/lib/wasm \
  --out-name simon_verify \
  target/wasm32-unknown-unknown/release/simon_verify_wasm.wasm

echo "== wasm generated: ui/src/lib/wasm/simon_verify_bg.wasm =="
ls -la ui/src/lib/wasm
