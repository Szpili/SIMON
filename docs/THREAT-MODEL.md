# SIMON — Threat Model & Claim Matrix

**Scope:** this document fixes what SIMON claims and, more importantly, what it does **not**
claim. Every public sentence about security must map to one of three labels:

| Label | Meaning |
|---|---|
| **Cryptographically proven** | Follows from Ed25519 signatures and content commitments. Verifiable offline, on a CPU. |
| **Empirically detected** | Only the result of a named, reproduced measurement. No general guarantee. |
| **Not proven** | Explicitly out of scope without a TEE. Never present it as a guarantee. |

Repositioning: SIMON is **verifiable inference-result publication**, not verifiable inference
**execution**. The receipt is an integrity/authorship commitment, not execution attestation.

## Cryptographically proven

1. The named operator signed the receipt (Ed25519 over the digest of the whole receipt).
2. The receipt binds the exact prompt token ids (tokenizer hash + client nonce + prompt digest)
   and the exact output token chain (M1).
3. The declared model id, runtime, precision, sampling commitment and `rng_seed` are committed.
4. The receipt cannot be modified after signing; replay is detected via the client nonce.
5. `simon verify` (CLI) and the WASM verifier recompute these commitments from the data a
   verifier holds — offline, no network, no GPU.

## Empirically detected (named measurements only)

1. **Gross fabrication** of output is detected by the reference-policy check: in our setup, a
   node returning unrelated output gave 13/30 tokens outside the verifier top-2 (margin 1.98) →
   HardFail.
2. **Same-family quantization substitution is NOT detected.** Q4-node vs Q8-verifier passed whole
   sequences (factual/code); honest Q4→Q4 runs also showed single deviations; mean NLL did not
   separate either. This is published as a **negative result**.
3. The CPU guard catches part of PII/secrets: rules PII 7/12, secrets 11/12 (0.1 ms); Bielik-Guard
   0.5B toxicity ~150 ms, clean false-positive 1/15. It does **not** catch prompt injection.

## Not proven (never claim)

1. That the declared model actually produced the output (no execution attestation).
2. That the declared quantization/runtime was used.
3. That the computation was honest, complete, or correct.
4. That no arbitrary code ran before inference, and no data was exfiltrated.
5. That the output is factually correct.
6. That prompt injection was prevented (the guard mitigates *some* content; authorization limits
   *what an injected instruction can do*, but neither is prevention).
7. That a stake or reputation makes dishonest behaviour impossible.

## Sentences we may publish

- "Every inference result is bound by an Ed25519 signature to the operator key, the prompt token
  digest, the declared model/tokenizer digest, and the declared execution profile and sampling
  parameters."
- "Anyone can verify *who* published *what* result for *which input* and *which declared*
  model/parameters — offline, on a CPU."
- "An optional reference-policy check detects gross trajectory fabrication under the evaluated
  setup; it does not detect model or quantization substitution."
- "Authorization limits what an injected instruction can do (capability ceiling + operator grant);
  resource limits bound the cost. This is mitigation of blast radius, not prompt-injection
  prevention."

## Sentences we must never publish

- "SIMON verifies the correctness of inference execution."
- "SIMON guarantees the claimed model actually ran."
- "SIMON prevents nodes from fabricating results or substituting models."
- "SIMON detects malicious model behaviour." / "SIMON provides TEE-level security without a TEE."
- "The receipt proves correct inference."
- "Encrypted prompts prevent prompt injection."

## Claim labels in one line

> Receipt = integrity + authorship + input/state binding (**proven**). Gross fabrications and some
> PII/secrets are caught in measured tests (**empirical**). Execution, model identity, correctness
> and non-interference are **not proven** without a trusted execution environment.
