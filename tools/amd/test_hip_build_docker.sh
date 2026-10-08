#!/usr/bin/env bash
# FREE pre-test: configure + partially build llama.cpp for HIP inside a ROCm image,
# WITHOUT --gpus. Compilation needs no GPU; this catches dependency/flag/cmake errors
# before they cost paid MI300X hours.
#
#   bash tools/amd/test_hip_build_docker.sh
# Overrides: ROCM_IMAGE (default rocm/dev-ubuntu-22.04:6.2-complete), LLAMA_CPP, AMDGPU_TARGETS.
set -euo pipefail
# Default = AMD Quick Start (recent ROCm + hipBLAS + vLLM). The base dev image 6.2 is too
# old for gfx942 fp8 (__hip_fp8_e4m3) and lacks hipBLAS — see README "Pre-test findings".
IMG="${ROCM_IMAGE:-rocm/vllm:latest}"
LLAMA="${LLAMA_CPP:-$HOME/llama.cpp}"
TARGET="${AMDGPU_TARGETS:-gfx942}"

command -v docker >/dev/null || { echo "docker MISSING"; exit 1; }
[ -d "$LLAMA" ] || { echo "no llama.cpp at $LLAMA"; exit 1; }

echo "== pulling $IMG (large; one-time) =="
docker pull "$IMG"

echo "== HIP configure + build (no GPU) =="
docker run --rm -v "$LLAMA":/src -w /src "$IMG" bash -lc "
  set -e
  export DEBIAN_FRONTEND=noninteractive
  command -v cmake >/dev/null || { apt-get update -qq && apt-get install -y -qq cmake >/dev/null; }
  # PATERN ZLAPANY PRZEZ PRE-TEST: baza dev-ubuntu nie ma hipBLAS, a ggml-hip go wymaga.
  ls /opt/rocm/lib/cmake/hipblas/hipblasConfig.cmake >/dev/null 2>&1 \
    || { apt-get update -qq && apt-get install -y -qq hipblas-dev rocblas-dev >/dev/null; }
  export ROCM_PATH=\${ROCM_PATH:-/opt/rocm}
  cmake -S /src -B /src/build-hip -DGGML_HIP=ON -DAMDGPU_TARGETS=$TARGET -DCMAKE_BUILD_TYPE=Release
  cmake --build /src/build-hip --config Release -j\$(nproc) --target llama-cli
  echo 'HIP BUILD OK (no GPU needed)'
"
