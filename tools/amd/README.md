# SIMON — AMD MI300X session runner

One command, in order, because credits are spent by the hour:

1. **FP arm** — llama.cpp built for HIP (`gfx942`), same GGUF, same 32 prompts, same
   `fp_matrix.py` as the local matrix (`docs/FP-PREREGISTRATION.md`).
2. **SIMON node on AMD** — for the demo and dashboard.
3. **Video** — record the session.
4. **Destroy the VM.**

The AMD arm is **one configuration** in the pre-registered matrix (different vendor), not a
separate project. It also satisfies the hackathon requirement that AMD is *inside the working
product* (phase 2 runs the SIMON node on the MI300X).

## Public claim (scoped)

> M3 **in its current form** (20-step window, threshold 0.02) is not a detector.

Do **not** move the threshold using this data — the pre-registration forbids it. The lesson is
that a 20-step window cannot resolve anything finer than one token.

## Pre-test before the 12th (free, no GPU)

Compiling llama.cpp for HIP does not need a GPU. A ROCm Docker image **without `--gpus`**
catches dependency and flag errors for free:

```sh
bash tools/amd/test_hip_build_docker.sh
```

## Session (on the MI300X VM)

```sh
export SIMON_REF_URL=http://<szpon-tailscale>:18201     # NVIDIA reference, reachable over tailnet
export SIMON_GGUF=/models/Qwen2.5-3B-Instruct-Q8_0.gguf
export SIMON_MODEL_HASH=qwen2.5-3b-instruct-q8
bash tools/amd/session.sh all                           # env -> build -> fp -> node -> video
bash tools/amd/session.sh destroy                       # when done (spends no more hours)
```

Phases can be run individually: `env | build | fp | node | video | destroy`.

## Pre-test findings (free, no GPU) — 2026-10-06

The Docker smoke test caught two real blockers before any paid hour:

1. **`hipBLAS` missing.** `rocm/dev-ubuntu-22.04:6.2` has no `hipblasConfig.cmake`, but
   `ggml-hip` requires hipBLAS. Fix applied: install `hipblas-dev rocblas-dev` when absent
   (or use the Quick Start image, which ships them).
2. **ROCm 6.2 too old for gfx942 fp8.** The build fails with
   `unknown type name '__hip_fp8_e4m3'` when compiling for `gfx942`. The local llama.cpp
   commit needs a newer ROCm / the Quick Start image.

**Therefore the default pre-test image is `rocm/vllm:latest` (AMD Quick Start: recent ROCm +
hipBLAS + vLLM preinstalled).** Detect, do not install. Re-run before the 12th:

```sh
ROCM_IMAGE=rocm/vllm:latest bash tools/amd/test_hip_build_docker.sh
```

## Use what the image has

The runner **detects** an existing ROCm (`/opt/rocm`) and vLLM instead of installing its own;
`env.sh` prints what it found. Only if HIP llama.cpp is absent does `build_hip.sh` build it.

## Files

| file | purpose |
|---|---|
| `env.sh` | detect ROCm / vLLM / GGUF / binaries; print a report |
| `build_hip.sh` | build llama.cpp for HIP, `AMDGPU_TARGETS=gfx942` (or `$AMDGPU_TARGETS`) |
| `fp_arm.sh` | start the HIP `llama-server`, run `fp_matrix.py` against `$SIMON_REF_URL`, collect JSONL |
| `simon_node.sh` | run the SIMON node on AMD (endpoint = the HIP server) |
| `video.sh` | record the session (`ffmpeg` x11grab or a documented fallback) |
| `destroy.sh` | teardown reminder + best-effort local cleanup |
| `test_hip_build_docker.sh` | free HIP build smoke test in a ROCm image, no GPU |
| `session.sh` | orchestrator |

## Notes

- Transports the same `$SIMON_GGUF` and the same `prompts_fp.txt`; tokenizer must match the
  reference (the runner checks via `fp_matrix.py`).
- If the AMD box cannot reach the NVIDIA reference, run phase 1 the other way instead: drive
  `fp_matrix.py` from Szpon with `--cfg amd=http://<amd-tailscale>:18301`, and keep only the
  HIP server running on AMD.
