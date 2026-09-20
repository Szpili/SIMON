# SIMON Security Log

Here we record **what was wrong and since when** — with a description of the
previous behavior, not just the fix. The reader must be able to assess exactly
what the receipts issued before a given change proved.

---

## 2026-09-18 — P0: the receipt did not bind the response content

**Behavior before the fix.** `output_digest` was a digest of METADATA:

```
signature → job_id + counters + timing
output    → outside the signed commitment
```

The response itself went alongside the receipt, unsigned.

**Consequence.** A node could compute nothing and return arbitrary text: the
Ed25519 signature remained valid, `job_id` and `model_hash` matched, so every
client-side verification gate lit up green. The same applied to substitution in
transit or by the coordinator, and to splicing a receipt from a different
execution onto the current text.

**What the documentation claimed.** The demo and the whitepaper stated outright
that the signature covers the digest of the result. **That statement was
untrue.**

**Fix.**

```
signature → receipt containing a commitment to the output
client    → recomputes the commitment from the received content
```

- `output_digest` = `H("SIMON/OUTPUT/v1" ‖ job_id ‖ output)` — a domain
  separator and binding to the job, so a valid receipt cannot be substituted
  under the same text in a different job;
- the client gained a fourth gate: it recomputes the digest from what it
  actually received;
- `--verify-receipt --expect-output <plik>` lets someone who received the result
  and the signature from a third party check this offline;
- regression tests for exactly this attack (content substitution,
  one-character substitution, inflating counters after signing, the domain
  separator).

**What this fix closes:** substitution of the response by the node, substitution
in transit or by the coordinator, splicing a receipt from a different execution
onto the current text, modification of the result after signing.

**What it does NOT close:** it does not prove that the declared model generated
the signed content. A malicious node can sign arbitrary text together with a
correctly computed digest. Only the execution audit is to bind
`model + prompt + execution + output`.

**A receipt after the fix proves exactly this:**
> A registered node signed exactly this response as the result of this job.

**And it still does NOT prove:**
> The declared model generated this response.

---

## 2026-09-18 — timing: there is NO resistance to manipulation

**Behavior before the fix.** `gen_ms` was computed as
`tokens_out / 65.0 * 1000` — from a constant measured at some point for Bielik
11B on a 3080 Ti, applied to EVERY node and model. The comment next to it
claimed this was a "hard measurement, not a tok/s heuristic" — exactly the
opposite of what the code did.

**Consequence.** Throughput computed from this field came out **always
65.0 tok/s**, regardless of hardware and model. The demo displayed this number
as a measurement.

**Fix.** `gen_ms` is now the actually measured duration of the backend call
(without streaming it covers prefill and decoding together — they cannot be
separated). After the fix, the same prompt takes 910 ms on vLLM/3090 and
7798 ms on Ollama/3080 Ti.

### Threat model: timing manipulation

**We have no resistance whatsoever, and it cannot be obtained with signatures
alone.**

| Layer | Who measures | What the signature guarantees | Can it be lied about |
|---|---|---|---|
| `node_declared_ttft_ms`, `node_declared_gen_ms` | the node about itself | only that it does not change after the fact | **yes, arbitrarily** |
| `ClientObservationV1` | the client about itself | only that it does not change after the fact | **yes, arbitrarily** |
| client/node difference | two parties with opposing interests | nothing | **yes, under collusion** |

In addition, `started_at_us` / `finished_at_us` in the receipt come from the
node's wall clock — settable arbitrarily and, with NTP, also jumping. The client
uses a **monotonic** clock, so it is resistant to jumps, but not to lying.

**Why anyone would manipulate timing:** to look faster and win allocation, to
look slower and justify a higher charge, or to give a plausible duration to
fictitious work under wash-compute.

**What real resistance would require.** Time cannot be proven cryptographically
between parties that do not trust each other. Approximations: hardware
attestation (TEE), independent observers sampling the node (statistics, not
proof), or deriving the expected time from an audited execution and a known
hardware profile — still hardware-dependent.

**Practical position, in force from now on:** time is NOT the basis for
settlement, slashing, performance ranking, or dispute resolution. It serves
exclusively as an operational signal. Settlement is to be based on token counts
(only once they are independently recomputable — M5.1b) and on the execution
audit.

## 2026-09-18 — audit of all constants

The conclusion from the `65.0` constant incident: if one number pretending to be
a measurement survived the code, the comment, and the demo, one must not assume
it is the only one. A review of all constants in `crates/`, each one classified.

| Constant | Location | Classification | Action |
|---|---|---|---|
| `tokens_out / 65.0 * 1000` | `node.rs` | **FAKE_MEASUREMENT** | removed — real measurement |
| `ZNAK_NA_TOKEN_WORST_CASE = 1.2` | `mapreduce.rs` | CALIBRATION_CONSTANT | stays — it has a source, a date, and an explicit "this is not a formal guarantee" |
| `ZNAK_NA_TOKEN_PROZA_GENEROWANA = 3.5` | `mapreduce.rs` | CALIBRATION_CONSTANT | stays — source + the incident that forced it |
| `narzut_szablonu_tokenow = 60` | `agent.rs` | **ASSUMPTION without a source** | named `NARZUT_SZABLONU_TOKENOW`, described as an assumption together with the consequence of an error |
| `empiryczny * 0.9` | `agent.rs` | unnamed margin | named `MARGINES_ZAOSTRZENIA` |
| `chars / 3.5` (inline) | `agent.rs` | duplicate of a constant | replaced with a named constant — risked divergence |
| `max_tokens 128`, `chunk_tokens 4000`, `map_max_tokens 200` | `main.rs` | CONSTANT_CONFIG | stay — explicit CLI defaults |
| `TIMEOUT_ODPOWIEDZI 300 s`, `REQUEST_TIMEOUT` | `agent.rs`, `harness` | CONSTANT_LEGITIMATE | stay |

The field `tok_s` was also renamed to `node_declared_tok_s`: it is computed from
the time declared by the node, so the name has to say so.

### The test that was missing

The bug passed through the code, the comment, and the demo, even though **the
signal was visible**: two different cards showed the same number. A test for a
suspiciously constant value was missing. Added:

- `simon_core::pomiar::podejrzanie_identyczna` + tests on historical data:
  the pair (65.0; 65.0) is caught, the pair measured after the fix (44.0; 3.3)
  passes;
- `scripts/sprawdz_profile.sh` — an invariant on LIVE nodes: two different
  profiles cannot yield the same throughput, and a node cannot declare a time
  LONGER than that measured at the client. Run of 2026-09-18:
  vLLM/3090 64.94 tok/s, Ollama/3080 Ti 6.50 tok/s — invariant satisfied.

A side note worth remembering: the constant 65 was **roughly correct for one
node** (measured 64.94 tok/s). That is precisely why nobody questioned it — a
false measurement that agrees in one case is harder to detect than obvious
nonsense.

### Wall clock degraded

`Receipt::elapsed_secs()` computed the duration from the difference of
`started_at_us` and `finished_at_us` — two timestamps from the node's wall
clock. Besides the fact that the node can set them arbitrarily, NTP can step
time backwards, so the difference is sometimes random or (via `saturating_sub`)
zeroed out, which masks the problem. The method is now marked `#[deprecated]`,
and the fields remain solely as an approximate marker for the log. **Duration is
measured with a monotonic clock on the side that is measuring. We never
subtract timestamps from two machines.**

### Hierarchy of timing signals

```
client_observed_total_ms   — the strongest available signal
node_reported_gen_ms       — weak, diagnostic only
node wall-clock timestamps — the weakest, binding for nothing
```

Node times are even weaker than the client's observation, because they concern
the inside of a process that nobody observes from the outside. The client at
least measures real transport time.

## 2026-09-18 — client identity was ephemeral

**Behavior before the fix.** The agent called `Keypair::generate()` on EVERY
startup. Two startups were two different people.

**Consequence.** The M5.2 metric — announced the previous day as done — **could
not attribute work to anyone**: the `client_pubkey` field was a different random
key in every record. Proof from a live file: 2 records, 2 keys. The bug was
found only when the question of identity recovery came up: it turned out there
was nothing to recover.

**Fix (M5.2a).** `simon_core::tozsamosc`: a persistent key in the user data
directory, atomic write (temporary file in the same directory → `fsync` →
`0600` → `rename` → read back → comparison of the public key),
`--identity-file` to explicitly point at a different path.

**The most important rule here: we never regenerate the key after an error.**
A corrupted file, overly broad permissions, or an inability to write **halt
startup**. Silent regeneration would look like a successful start while quietly
creating a new identity and again splitting the metric — the same bug, only
harder to notice.

The secret is not accepted on the command line or in an environment variable (it
would be visible in `ps` — see the `--key` incident of the same day) and does
not reach any log. `Keypair` deliberately does not expose the secret and has no
`Debug`.

**Old records remain untouched.** They carry `identity_epoch: 0` =
`LEGACY_EPHEMERAL_IDENTITY`, `ownership: UNRECOVERABLE`. They prove that a
specific ephemeral key signed work, but the new identity cannot
cryptographically prove that it controlled those keys — **so we do not rewrite
them**.

**Verified live:** 2 jobs → 1 client key (before: 2 → 2), file `0600`, zero hits
of the secret in the agent's output. Eight unit tests, including: a corrupted
file does not cause silent regeneration, an interrupted write does not destroy
the previous key, overly broad permissions halt startup, the error message does
not contain the secret.

**What is deliberately NOT here:** recovery phrases, rotation, revocation, and
derivation of multiple roles from a single root. That requires a frozen format
and test vectors — M5.2b/M5.2c in `ROADMAP.md`.

## 2026-09-18 — a change in digest format invalidated older receipts

Adding the domain separator `SIMON/OUTPUT/v1` changed how `output_digest` is
computed. Receipts issued earlier the same day **do not pass** verification with
the current binary — and **rightly so**, because the commitment is computed
differently.

**The problem is not in the behavior, but in the message.** The receipt does not
carry a schema version, so an old receipt is rejected as "content mismatch,"
while the real reason is an unknown format. Someone diagnosing this without this
note would look for content substitution that never happened.

Caught while putting an example receipt into the repo: the example did not pass
its own verification. Fixed with a fresh receipt; **schema versioning remains as
M5.1c** (`ReceiptV2` is to carry `schema`, and the old format is to get the
status `LEGACY_OUTPUT_BOUND`, not a silent content error).

## Verification status — as of 2026-09-18

```
Output binding:                   DONE + TESTS
Token fields signed:              DONE
Token counts recomputed:          PARTIAL / to be checked separately
Execution correctness:            NOT DONE
Economic credit:                  NOT DONE
Wash-compute resistance:          UNSOLVED
Node timing signed:               DONE
Node timing true:                 UNPROVEN
Client total time:                MEASURED (since 2026-09-18)
Client TTFT:                      STREAMING ONLY (none)
Timing used economically:         FORBIDDEN
Receipt schema split:             DEFERRED TO M5.1c
Audit of constants:               DONE (1 FAKE_MEASUREMENT, 3 unnamed)
Test for constant measurements:   DONE (unit + on live nodes)
Wall clock as time:               DEGRADED to log (#[deprecated])
Client identity:                  PERSISTENT (M5.2a)
Identity recovery:                NOT DESIGNED (M5.2b/c)
Client correlation:               MVP_IDENTITY, privacy not designed
```

### Known limitations, named outright

**Token counts are the node's declaration.** `prompt_tokens` and
`completion_tokens` are signed, so they cannot be changed after the fact — but
nobody has recomputed them independently. The target contract: the client sends
`input_token_ids` and `prompt_commitment`, the node returns
`ordered_output_token_ids`, and the client checks
`len(token_ids) == completion_tokens` and `decode(token_ids) == text`. Today
none of our backends exposes `token_ids` through the OpenAI API.

**The commitment is over the decoded text, not over `token_ids`.** Different
token sequences can decode to the same text, and inference verification concerns
the sequence actually processed by the model. The target commitment is also to
cover `finish_reason`, tool calls with arguments, multimodal results, the
variant at `n > 1`, the model and tokenizer manifest, the sampling parameters,
and the receipt schema version.

**The times `ttft_ms` and `gen_ms` are not trusted.** They come from the node;
the signature prevents changing them later, but does not make them true. Only
the round-trip time on the client side is measured independently. **Settlements
must not be based on time reported by the node.**

**Serialization is not canonical in the RFC 8785 sense.** `content_digest` uses
`serde_json::to_string`. Reproducible for this implementation (maps are
`BTreeMap`, floats forbidden in signed content since 2026-09-17), but a second
implementation of the protocol may compute a different byte stream.

### Killer tests — coverage status

| # | Test | Status |
|---|---|---|
| 1 | substitution of a single byte of the text | **present** |
| 2 | substitution of a single token ID | not feasible — no token IDs in the commitment |
| 3 | same text, different token ID sequence | not feasible — as above |
| 4 | substitution of the order of tool calls | not feasible — outside the commitment |
| 5 | substitution of a tool call argument | not feasible — as above |
| 6 | substitution of `finish_reason` | not feasible — as above |
| 7 | receipt from job A with the output of job B | **present** |
| 8 | same output, different prompt commitment | not feasible — no prompt commitment |
| 9 | inflated `completion_tokens` | **partial** — we catch a change AFTER signing, not a lie at signing |
| 10 | inflated `prompt_tokens` | **partial** — as above |
| 11 | different tokenizer revision | not feasible — no tokenizer manifest |
| 12 | reuse of a receipt (replay) | not feasible — no registry |

The "not feasible" entries are not deferred for convenience: each requires a
field the protocol does not have today. They are laid out in M5.2 in
`ROADMAP.md`.
