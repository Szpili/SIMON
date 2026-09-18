# SIMON

**Verifiable LLM inference on untrusted, heterogeneous hardware.**

SIMON is a peer-to-peer harness that dispatches inference jobs to nodes nobody
controls centrally, and then makes the result *checkable*: every job produces an
Ed25519-signed receipt, and any verifier can re-run a single forward pass over
`prompt + output` to audit what the node claimed it computed.

The hard question is not "can we distribute inference" — it is **"how do you
punish a lying node without punishing an honest one?"** That is the question
this repository is actually about, and most of the work here is measurement
rather than architecture.

---

## Status

Working, measured, and openly incomplete. Nothing below is projected — every
number was produced on our own machines and the raw data lives in the repo.

| Component | State |
|---|---|
| libp2p transport (gossipsub + request/response, CBOR) | working |
| Ed25519 job receipts | working |
| Admission control (C+1 gate) with real tokenizer counts | working |
| Chunking + hierarchical map-reduce for over-context jobs | working |
| Agent↔agent orchestration (job B consumes job A's output) | working |
| Slashing rules for dishonest nodes | **unresolved — see below** |

Platforms proven end-to-end: **Linux (CUDA)** and **macOS (Apple Silicon)**,
including cross-platform interop between them.

## What a receipt proves — and what it does not

Stating this precisely matters more than stating it favourably.

A receipt today proves exactly: **a registered node signed this exact answer as
the result of this job.** The signature covers a commitment to the output text
bound to the job id, so substituting the answer, tampering with it in transit,
or pairing a valid receipt from another run with this text are all rejected.

It does **not** prove that the declared model generated that answer. A malicious
node can sign arbitrary text together with a correctly computed digest. Binding
`model + prompt + execution + output` requires an execution audit (TopLoc), and
that is not implemented.

| | |
|---|---|
| Output binding | implemented + tested |
| Token fields signed | implemented |
| Token counts independently recomputed | partial — verify separately |
| Execution correctness | **not implemented** |
| Economic credit | **not implemented** |
| Wash-compute resistance | **unsolved** |

The token counters are the node's signed *declaration*: the signature stops
them being changed afterwards, it does not make them true. The same holds for
the reported timings — only round-trip latency is measured independently, so
**settlement must never rest on node-reported time**. `docs/SECURITY-LOG.md`
records the previous behaviour of each fix, not just the fix.

## Measured results

**Audit cost.** An audit is `prefill(N+M)`; the job itself is
`prefill(N) + decode(M)`. Decoding is ~55× slower per token, so auditing is
cheap:

| prompt | output | job | audit | audit as % of job |
|---:|---:|---:|---:|---:|
| 500 | 50 | 7.9 s | 0.63 s | 8.0% |
| 2016 | 200 | 18.8 s | 1.86 s | 9.9% |
| 2011 | 500 | 87.1 s | 2.07 s | 2.4% |
| 6037 | 500 | 69.1 s | 5.12 s | 7.4% |

**2.4–9.9% of the job, median 7.4%.** This is the number that decides whether
verification can be paid for out of the job price. It can.

**Honest drift.** Before you can slash a node for a mismatched fingerprint, you
must know how much two *honest* runs differ. We measured it. On identical
hardware the token sequence was stable and logprob deltas were tiny — but we
explicitly **withdrew** two stronger claims we had first drawn from that data,
because the measurement could not support them (see
`docs/design/DESIGN-VERIFIER-0.md`). A cross-hardware run came back
bit-identical, which is a *confounded* negative result, not a green light.

**Therefore: a fingerprint mismatch is currently treated as a reason to
re-audit, not as proof of fraud.** Automatic slashing is not enabled, and we say
so rather than shipping a threshold we cannot defend.

## Why this matters for AMD

SIMON's entire value proposition is running on hardware that is *not* uniform —
whatever GPU a volunteer happens to own. But our verification work so far has
been proven on NVIDIA/CUDA and Apple Silicon only, for the mundane reason that
those are the machines we have.

That is a real gap, and it is precisely where the interesting question lives:
**numerical reproducibility across vendors.** If an honest AMD node and an
honest NVIDIA node compute measurably different activations for the same input,
then any naive fingerprint comparison slashes honest participants — and every
cross-vendor decentralized inference network has this problem whether or not it
has noticed yet.

We intend to measure it on ROCm rather than speculate about it. The node backend
talks to an OpenAI-compatible endpoint, so a ROCm-served model is a
configuration change, not a rewrite. What has not run yet is the ROCm arm of the
comparison itself — that is the next measurement, on AMD Developer Cloud.

## Build

```bash
cargo build --release
```

Rust workspace, no external services required to build. Running a node
additionally needs an OpenAI-compatible inference server (we use vLLM on Linux,
llama.cpp elsewhere).

## Documentation

- `docs/whitepaper/SIMON-Whitepaper-EN.pdf` — full write-up (EN; PL version alongside)
- `docs/design/DESIGN-VERIFIER-0.md` — verifier economics, drift measurements, and the open questions
- `ROADMAP.md` — what is done, what is blocked, and by what
- `notebooks/SIMON-D2-ROCm.ipynb` — the cross-vendor measurement itself, runnable on
  AMD Developer Cloud; the CUDA baseline is embedded, so it reports a verdict in-session

## Licence

MIT.
