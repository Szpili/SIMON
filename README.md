# SIMON — Sovereign Inference & Model Orchestration Network

**Your models. Independent nodes. Inference on demand.**

Verifiable LLM inference on untrusted, heterogeneous hardware. "Sovereign" means you choose whose GPUs run
your jobs (your own, a partner's, or rented cloud) and every answer comes back signed by the node that wrote
it. It does not mean the prompt stays private: the node that runs a job sees it in plain text.

SIMON is a peer-to-peer harness. It sends inference jobs to nodes that no one
controls centrally, and it makes each result verifiable. Every job produces a
receipt: a record that the node signs with Ed25519. Any verifier can audit a
receipt. An audit is one forward pass over `prompt + output`, and it tests what
the node claimed it computed.

The hard question is not "can we distribute inference". The hard question is
"how do you punish a lying node without punishing an honest one?". This
repository is about that question. Most of the work here is measurement, not
architecture.

## Hackathon entry: maintenance tickets with an audit trail (Intelligent Industry, Gemma 4)

**[Live: /simon/industry](https://pop-os.tailaf1556.ts.net/simon/industry)**. Static mirror that does not depend on our
machine: [szpili.github.io/SIMON/industry](https://szpili.github.io/SIMON/industry) (the same signed tickets, verified in your browser). The user is the maintenance
planner of a machining plant. Machine alarms become technician tickets by hand, late, and nobody can later
prove which system produced which advice.

1. Deterministic rules decide the risk and the failure mode (the published physics of the AI4I 2020
   milling-machine dataset, `tools/simon-industry/rules.py`). The language model never decides it.
2. **Gemma 4 26B-A4B** drafts the technician ticket as JSON from the rule findings only (title, summary, and
   inspection steps picked by ID from a fixed per-mode catalog; it never writes a step), as a SIMON job on
   the plant's RTX 3090 (QAT q4_0, llama.cpp) or on an **AMD Instinct MI300X** in AMD Developer Cloud
   (BF16, vLLM 0.27.1 on ROCm).
3. Every ticket is checked: exact fields, failure modes and urgency equal to the rules, steps only from the
   catalog, short free text with no number the input lacks, no link, role marker, urgency word or instruction
   to stop or bypass equipment, marked for human review. A ticket with any problem is held for a person,
   not published. The stress cases run in CI (`tools/simon-industry/stress_corpus.json`).
4. The node signs the exact ticket text. The page re-checks every receipt in the browser.

Result on a predeclared, stratified sample (`docs/industry/run-catalog-3090-mi300x-2026-10-17.jsonl`): 14 of
15 tickets published, 1 held (its summary said "Immediate inspection" while the rules said "inspect before next
shift"; that check was added after seeing it), median 1.6 s (1.37 s on the RTX 3090, 1.64 s on the MI300X), all receipts valid; 8 written on the RTX 3090,
7 on the MI300X. Each receipt carries the node's measured runtime, precision and execution time, and the
gateway pins each node's signer key (run of 2026-10-17, after the receipt fixes; the 2026-10-15 run gave the same 14/15).
**Measured improvement (safety and quality).** A ticket draft takes 1.6 s (median) on the plant's own GPU or on
the MI300X. In a teammate's stress test, the first validator (a keyword denylist) accepted 7 of 12 unsafe drafts.
After the redesign (steps only from the plant's catalog, evidence copied from the rules, short checked free text,
any problem holds the ticket), all 15 reject cases in the CI stress corpus, modelled on those attack classes, are
held. One documented known-limit case passes (unsafe meaning in plain words), which is why a technician reviews
every ticket. On the live sample, 1 of 15 drafts was held for raising the urgency the rules had set.
Labelled failures the rules miss are shown as misses. AI4I is synthetic, and its labels follow the same
physics as the rules, so the rule recall describes the dataset, not a real plant.

```bash
python3 tools/simon-industry/rules.py tools/simon-industry/data/ai4i2020.csv      # rule recall per failure mode
python3 tools/simon-industry/tickets.py --csv tools/simon-industry/data/ai4i2020.csv \
    --model gemma-4-26b-a4b-it-qat-q4_0 --model gemma-4-26b-a4b-it-bf16 --out run.jsonl   # through the gateway
bash tools/amd/demo_node.sh up <droplet-ip> <name>    # a fresh MI300X droplet -> Gemma 4 SIMON node in the demo
```

Try it:

- [Industry](https://pop-os.tailaf1556.ts.net/simon/industry): the tickets above, each receipt verified
  in your browser.
- [Tamper, then verify](https://pop-os.tailaf1556.ts.net/simon/demo): change the
  answer of a node and watch the receipt fail. The verifier runs in your browser.
  It is the same Rust verifier as the CLI, compiled to WebAssembly.
- [Network](https://pop-os.tailaf1556.ts.net/simon/network): live node cards and
  the job feed. Your browser verifies the signature of every receipt. The page
  shows data only while our demo node is up. It never shows sample data.
- `przyklady/README.en.md`: verify a real receipt offline with one command.

---

## Architecture

1. A client (the `simon --role agent` CLI, or the OpenAI-compatible gateway in `tools/simon-api`) signs a
   job order and sends the prompt to one node over libp2p request-response. Only that node sees the prompt.
2. The node (`simon --role node`) runs the job on any OpenAI-compatible backend (vLLM, llama.cpp, a hosted
   API) and returns the output with an Ed25519-signed receipt. The receipt binds the job, the output text
   and the tool calls.
3. The client verifies the receipt offline before it uses the answer (`simon --verify-receipt`, or in the
   browser through the same Rust verifier compiled to WebAssembly).
4. A verifier (`tools/simon-lidar`, pilot) checks which model a node serves, from the node's behaviour.
   A receipt shows who signed an output. It does not show which model produced it.

## Status

The software works. We measured every number below on our own machines, and
the raw data is in the repository. Nothing below is a projection. The table
also names what is not done.

| Component | State |
|---|---|
| libp2p transport (gossipsub + request/response, CBOR) | working |
| Ed25519 job receipts | working |
| Admission control (C+1 gate) with real tokenizer counts | working |
| Chunking + hierarchical map-reduce for over-context jobs | working |
| Agent↔agent orchestration (job B consumes job A's output) | working |
| OpenAI-compatible API gateway (`tools/simon-api`): each completion is a real SIMON job with its receipt | working |
| Network monitor (`/network` page + `tools/simon-monitor`) | working |
| Integrity canary for corrupted weights (GPUs without ECC, GPUHammer): operator self-check, `tools/simon-canary` | working; catastrophic corruption only, see `docs/INTEGRITY-CANARY.md` |
| Tool calls and streaming through the gateway: coding agents (Cline, opencode) work through SIMON, and the receipt binds tool names, arguments and `finish_reason` | working, tested end to end (`tools/simon-api/check_tools.py`) |
| Concurrent jobs on one node (`--max-jobs`, a queue, an explicit busy error) | working, measured on MI300X |
| Model-identity verifier: agent behaviour (LIDAR-style) + single-token output distributions (`tools/simon-lidar`) | pilot, small N: family and size separate, quantization only in the single-token layer, no false alarm on another GPU (`docs/VERIFIER-PILOT.md`) |
| AMD ROCm node (MI300X) | working and measured (see below) |
| Gemma 4 26B-A4B nodes: RTX 3090 (QAT q4_0, llama.cpp) and AMD Instinct MI300X (BF16, vLLM 0.27.1, ROCm) | live in the demo; MI300X telemetry from rocm-smi on `/network` |
| Maintenance tickets with an audit trail (`tools/simon-industry`, `/industry`) | working: 14/15 published, 1 held for review, on a predeclared sample |
| Slashing rules for dishonest nodes | unresolved (see below) |

Platforms proven end to end: Linux (CUDA), Linux (ROCm, AMD Instinct MI300X),
and macOS on Intel x86_64. Cross-platform interoperation between them is also
proven.

The Mac is a 2012 Core i7, and that is the point. On 18 September 2026 that
laptop verified a receipt that an RTX 3090 issued. The result was
`RECEIPT VALID`. Then we appended one space to the answer, and the result became
`REJECTED`. The laptop needed no GPU, no network, and no trust in the person who
handed over the files. You can reproduce it with `przyklady/README.en.md`. We
did not test Apple Silicon or big-endian targets, and we do not claim them.
`docs/PAZUR-macos-bringup.md` is the runbook (Polish).

## What a receipt proves and what it does not

A precise statement matters more than a favorable one.

A receipt today proves exactly this: a registered node signed this exact answer
as the result of this job. The signature covers a commitment to the output
text, bound to the job id. Therefore the verifier rejects a substituted answer,
an answer changed in transit, and a valid receipt from another run paired with
this text.

A receipt does not prove that the declared model generated the answer. A
malicious node can sign arbitrary text together with a correctly computed
digest. To bind `model + prompt + execution + output`, SIMON needs an execution
audit (TopLoc). That audit is not implemented.

| | |
|---|---|
| Output binding | implemented and tested |
| Token fields signed | implemented |
| Token counts independently recomputed | partial, recompute them separately |
| Execution correctness | not implemented |
| Economic credit | not implemented |
| Wash-compute resistance | unsolved (see `docs/WASH-COMPUTE.md`) |

The token counters are the signed declaration of the node. The signature stops
a later change of the counters. It does not make them true. The same holds for
the reported timings. Only round-trip latency is measured independently.
Therefore settlement must never rest on node-reported time.
`docs/SECURITY-LOG.en.md` records the previous behavior of each fix, not only
the fix.

## Measured results

### Audit cost

An audit is `prefill(N+M)`. The job itself is `prefill(N) + decode(M)`.
Decoding is about 55 times slower per token, so an audit is cheap:

| prompt | output | job | audit | audit as % of job |
|---:|---:|---:|---:|---:|
| 500 | 50 | 7.9 s | 0.63 s | 8.0% |
| 2016 | 200 | 18.8 s | 1.86 s | 9.9% |
| 2011 | 500 | 87.1 s | 2.07 s | 2.4% |
| 6037 | 500 | 69.1 s | 5.12 s | 7.4% |

The audit costs 2.4% to 9.9% of the job, median 7.4%. This number decides
whether the job price can pay for the audit. It can.

### Honest drift

A fingerprint is the numeric summary that an audit produces. Before you can
slash a node for a mismatched fingerprint, you must know how much two honest
runs differ. We measured it. On identical hardware the token sequence was
stable, and the logprob (log-probability) deltas were tiny. We first drew two
stronger claims from that data. We withdrew both in public, because the
measurement did not support them (see `docs/design/DESIGN-VERIFIER-0.md`). A
cross-hardware run came back bit-identical. That is a confounded negative
result, not a green light.

Therefore a fingerprint mismatch is today a reason to re-audit, not proof of
fraud. Automatic slashing is off. We say so, because we will not ship a
threshold that we cannot defend.

## Why this matters for AMD

The value of SIMON is that it runs on hardware that is not uniform: whatever
GPU a volunteer owns. But until October 2026 we measured audits on NVIDIA/CUDA
and on an Intel Mac only, for one reason: those were the machines we had.

That gap is where the interesting question lives: numerical reproducibility
across vendors. Suppose an honest AMD node and an honest NVIDIA node compute
different activations for the same input. Then a naive fingerprint comparison
slashes honest participants. Every cross-vendor decentralized
inference network has this problem, whether it noticed or not.

We measured it on ROCm instead of speculation (AMD Developer Cloud, 1× MI300X,
2026-10-11). The full write-up and the raw data are in
`docs/AMD-MI300X-RESULTS.md`.

- Cross-vendor false positives: in the pre-registered test, an honest MI300X
  flagged 5 of 32 prompts. The NVIDIA reference flagged the same 5 when we
  tested it against itself. The MI300X output was byte-identical on 22 of 32
  prompts. The vendor adds no false positives for this model and this metric.
  The noise comes from the audit method itself.
- SIMON node on AMD: Qwen3.8-27B on the MI300X, reached from Poland through the
  OpenAI-compatible gateway. The receipts verify, and a single changed byte is
  rejected.
- Capacity: one MI300X sustained 64 concurrent streams of Qwen3.8-27B (BF16,
  vLLM). All 192 requests had a time to first token (TTFT) of 3 s or less.
  Each stream ran at 10 tok/s or more, about 2000 tok/s in total.
- SIMON layer cost: on 2026-10-13 we sent the same jobs to vLLM directly and
  through a SIMON node on one MI300X. At 64 concurrent streams both paths ran
  at about 2000 tok/s. All 510 jobs (1 to 64 streams) came back with a verified
  receipt. One job took about 14 ms more than with vLLM alone.
- Routing, not sharding: one model split across a MI300X and a 3090 over the
  internet ran 12 times slower. Remote experts of a mixture-of-experts (MoE)
  model ran 55 times slower. Separate whole-model nodes gave lower latency, not
  more throughput. Therefore SIMON routes whole jobs. If a model does not fit
  on one node, SIMON splits it.

## Build

```bash
cargo build --release
cargo test --workspace      # unit + integration tests across 5 workspace crates, 0 compiler warnings (CI runs on every push)
```

SIMON is a Rust workspace composed of 5 crates (`crates/simon-core`, `crates/simon-verify-wasm`, `crates/simon-harness`, `crates/simon-node`, `crates/simon-cli`) alongside Python test and self-check suites (`tools/simon-industry/rules.py`, `tools/simon-canary/simon_canary.py`, `tools/simon-verifier/verifier_loop.py`). The build needs no external services. A node also
needs an OpenAI-compatible inference server. We use vLLM on Linux and llama.cpp
elsewhere.

```bash
# a node in front of any OpenAI-compatible backend
simon --role node --listen /ip4/0.0.0.0/tcp/9001 --model-url http://127.0.0.1:8000 \
      --model-hash my-model --key-file node.key
# OpenAI-compatible API in front of SIMON nodes (stdlib Python); jobs logged for the monitor
SIMON_API_NODES=nodes.json SIMON_API_JOBLOG=jobs.jsonl python3 tools/simon-api/simon_api.py
# read-only monitor JSON for the /network page
SIMON_MONITOR_CONFIG=monitor.json SIMON_API_JOBLOG=jobs.jsonl python3 tools/simon-monitor/simon_monitor.py
```

The top of each script documents its file formats.

## Documentation

- `docs/whitepaper/SIMON-Whitepaper-EN.pdf`: the full write-up (EN, with a PL version alongside)
- `docs/design/DESIGN-VERIFIER-0.md`: verifier economics, drift measurements, open questions (Polish)
- `ROADMAP.md`: what is done, what is blocked, and by what (Polish)
- `przyklady/README.en.md`: a real receipt that you can verify offline in one command
- `docs/AMD-MI300X-RESULTS.md`: the MI300X measurements: cross-vendor false positives, node + API, throughput, MoE, concurrency
- `docs/INTEGRITY-CANARY.md`: corrupted weights on GPUs without ECC (GPUHammer): measured single-bit flips, the canary, its limits
- `docs/VERIFIER-PILOT.md`: which model does a node serve? Two-layer verifier pilot, results and limits
- `docs/WASH-COMPUTE.md`: the wash-compute problem, a research proposal, and our review of it
- `docs/SECURITY-LOG.en.md`: every fix with the behavior it replaced, and the current status table
- `docs/PAZUR-macos-bringup.md`: the macOS bring-up runbook, with the traps (Polish)
- `notebooks/SIMON-D2-ROCm.ipynb`: the cross-vendor measurement itself. It runs on
  AMD Developer Cloud. The CUDA baseline is embedded, so it reports a verdict in one session.

## License

MIT.
