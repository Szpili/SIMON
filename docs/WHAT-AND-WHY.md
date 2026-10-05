# SIMON — What we are building and why

## The problem

LLM inference is increasingly done on machines nobody controls centrally — consumer GPUs,
old laptops, rented boxes. The hard question is not "can we distribute inference" but
**"how do you tell a lying node from an honest one without trusting the hardware?"**

There is no trusted execution environment (TEE) on most nodes, and Zero-Knowledge ML is
10^4–10^6× the cost of the inference itself, so it is out. What is left is:
cryptographic **binding** of what was said, plus **empirical detection** of some misbehaviour,
plus **economic** deterrence for the rest. This repository is about making those three layers
honest and measurable.

## The honest reframing

We do **not** claim "verifiable inference execution". We build **verifiable inference-result
publication**:

- **Cryptographically proven:** an Ed25519 receipt binds the operator key, the prompt-token
  digest (tokenizer hash + client nonce), the output-token chain, the declared model/tokenizer
  digest, and the declared execution profile + sampling commitment. Tampering, replay and
  grant-reuse are detected offline, on a CPU.
- **Empirically detected (named tests only):** a reference-policy check catches **gross**
  fabrication; the CPU guard catches part of PII/secrets/toxicity.
- **Not proven:** that the declared model actually ran, that the computation was correct or
  honest, that no code executed and no data left, or confidentiality vs the executor.

Never publish: "the model was executed", "the result is correct", "substitution is prevented",
"TEE-level security without a TEE".

## What is implemented (and tested)

| Layer | What | Where |
|---|---|---|
| M1 binding | tokenizer hash + client nonce + prompt-token digest + output-token chain + sampling commitment + rng seed + exec profile + receipt level | `simon-core::receipt` (`WiazaniaM1`) |
| Verifier | one source of truth in Rust, also compiled to WASM for the browser | `simon-core` + `simon-verify-wasm` |
| Tamper-evidence | every bound field is under the signature; legacy = authorship-only | `tests/tamper_matrix.rs` |
| I2 capability ceiling | the LLM fills slots; every parameter validated; unknown actions rejected | `simon-core::cap` |
| I3 operator grant | crypto-bound to the operator key, exact action + approved parameter values, TTL, single-use, revocable | `simon-core::cap` |
| I4 information flow | `Trust`×`Sink`, deny-by-default, operator declassification | `simon-core::przeplyw` |
| I5 resource budgets | deadline, token/byte/tool-step budgets, cancellation; deterministic failure | `simon-core::limity` |
| Protocol hardening | nonce/replay window, token-bucket rate limit, message-size bounds | `simon-core::protokol` |
| Injection corpus | 20 adversarial + 20 benign + IFC flows against the authority boundary | `tests/injection_corpus.rs` |
| M3 diagnostic | reference-policy / trajectory-consistency check (top-k containment) — **diagnostic, not a gate** | `simon-core::m3` |
| Small CPU guard | deterministic PII/secret rules + Bielik-Guard 0.5B toxicity | `tools/simon-guard/` |
| Verifier loop | follows a node's trajectory through llama.cpp to score it | `tools/simon-verifier/` |
| CLI | `simon verify` (receipt), `--tokens` (M1), `--audyt` (M3), `guard` (CPU guard) | `crates/simon-cli` |

## The measurements that set the plan

1. **E0 — exact-match is dead.** At temperature 0 the same model+seed is **not** portable across
   backends (CPU vs GPU diverge in 4/4 prompts, from ~3 tokens; GPU vs GPU is bit-identical).
2. **Substitution matrix (negative result).** Q4-node vs Q8-verifier of the same model was **not**
   separated by top-k containment or mean NLL; honest runs also show single deviations. So M3
   detects gross fabrication only. We publish this negative result.
3. **Guard coverage.** Rules PII 7/12, secrets 11/12 (0.1 ms); Bielik-Guard 0.5B ~150 ms, clean
   false-positive 1/15. It does **not** catch prompt injection.
4. **Tamper matrix.** Every bound receipt field is under the signature (21 mutations all
   detected); a replayed nonce and a reused/expired/broadened grant are rejected.

## Why this order

If binding fails, every later layer (M3, IFC, staking) inherits forgery — so binding and its
tamper matrix come first. Then injection containment is about **authority**, not content: the
LLM may propose `log_message`, it cannot propose `shell`; untrusted text (peer/RAG/tool/model)
cannot become an instruction or an action argument without an explicit operator grant. Resource
budgets bound the DoS surface. The economic layer prices only **provable** protocol violations,
never "your model was different" (unprovable without a TEE) and never a diagnostic signal.

## What is explicitly not done yet

- A larger injection corpus (≥200) exercising **broad-scope** tools.
- A same-tokenizer size-substitution experiment (Qwen2.5-7B vs 14B) with a pre-registered AUROC.
- A genuinely independent backend (Transformers/PyTorch vs llama.cpp); same backend twice only
  checks determinism/replay, which is still worth doing first.
- Eclipse protection / gossip provenance with concrete libp2p knobs.
- Economic staking: design only, and only for decidable violations.

See `docs/THREAT-MODEL.md`, `docs/OPEN-QUESTIONS.md`, `docs/EKONOMIA-STAKING.md` and `ROADMAP.md`.
