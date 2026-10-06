#!/usr/bin/env bash
# Build llama.cpp for ROCm/HIP. Compilation needs no GPU (safe to test in Docker first).
set -euo pipefail

ROCM="${ROCM_PATH:-/opt/rocm}"
GPU_TARGET="${AMDGPU_TARGETS:-gfx942}"        # MI300X
LLAMA="${LLAMA_CPP:-$HOME/llama.cpp}"
BUILD="$LLAMA/build-hip"

[ -d "$ROCM" ]      || { echo "no ROCm at $ROCM (set ROCM_PATH)"; exit 1; }
[ -d "$LLAMA" ]     || { echo "no llama.cpp at $LLAMA (set LLAMA_CPP)"; exit 1; }
command -v cmake >/dev/null || { echo "cmake MISSING"; exit 1; }

echo "== configuring llama.cpp (HIP, $GPU_TARGET) =="
cmake -S "$LLAMA" -B "$BUILD" \
  -DGGML_HIP=ON \
  -DAMDGPU_TARGETS="$GPU_TARGET" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_C_COMPILER="$ROCM/llvm/bin/clang" \
  -DCMAKE_CXX_COMPILER="$ROCM/llvm/bin/clang++"

echo "== building llama-server (+cli) =="
cmake --build "$BUILD" --config Release -j"$(nproc)" --target llama-server llama-cli

echo "== done: $BUILD/bin/llama-server =="
ls -la "$BUILD/bin/llama-server"
