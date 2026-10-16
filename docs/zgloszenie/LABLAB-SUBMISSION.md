# lablab submission text — SIMON (AMD Developer Hackathon: ACT III)

**Track:** Intelligent Industry (Vibe Generation) · **Partner technology:** Google Gemma (Gemma 4)

## Project title

SIMON — maintenance tickets with an audit trail, on Gemma 4 and AMD Instinct

## Short description

A plant's monitoring rules flag a machine; Gemma 4 drafts the technician ticket on the plant's own GPUs or on
an AMD Instinct MI300X; every ticket carries a signed receipt you can verify offline in your browser.

## Long description

**User and problem.** The maintenance planner of a machining plant turns machine alarms into tickets by hand,
late, and cannot later prove which system produced which advice. Once AI writes those tickets, the plant needs
two things: the model must never be the one deciding whether a machine is at risk, and every piece of advice
must be traceable to the machine that produced it.

**What SIMON does.** SIMON is a peer-to-peer inference network for GPUs you already own plus cloud GPUs on
demand. Every job returns an Ed25519-signed receipt bound to the exact output; anyone can verify it offline,
including in the browser through the same Rust verifier compiled to WebAssembly. Teams call it through an
OpenAI-compatible API.

**The Intelligent Industry product (live).**
1. Deterministic rules (the published physics of the AI4I 2020 milling-machine dataset) flag the risk and the
   failure mode. The language model never decides it.
2. Gemma 4 26B-A4B drafts the technician ticket as JSON from the rule findings only: title, summary, and
   inspection steps picked by ID from the plant's fixed catalog. It never writes a step itself. Hosted on the plant's local
   RTX 3090 (QAT q4_0, llama.cpp) or on an AMD Instinct MI300X (BF16, vLLM 0.27.1 on ROCm).
3. Each ticket is checked: exact fields, failure modes and urgency equal to the rules, steps from the catalog,
   short free text with no new number, link, role marker or urgency word, marked for human review. A ticket
   with any problem is held for a person, not published. A stress corpus of these attacks runs in CI.
4. The GPU node signs the exact ticket. The /industry page re-checks every receipt in the browser.

Result on a predeclared, stratified sample (`docs/industry/run-catalog-3090-mi300x-2026-10-17.jsonl`): 14 of 15 tickets published and 1 held (its summary escalated the
urgency the rules had set), median 1.6 s per ticket (1.37 s on the plant's RTX 3090, 1.64 s on the MI300X in AMD Developer Cloud), all receipts valid (8 written on RTX 3090, 7 on MI300X). Labelled failures the rules miss are shown as misses, not hidden.

**Measured improvement.** On our adversarial corpus the unsafe-draft escape rate fell from 7 of 12 (the first,
keyword-based validator, in a teammate's stress test) to 0 of 15 after the redesign; one documented known limit
(unsafe meaning in plain words) is why a technician reviews every ticket. This is a software safety metric, not yet
a measured plant outcome. A draft takes 1.6 s median.

**AMD & Architecture.** Gemma 4 runs on an AMD Instinct MI300X (vLLM 0.27.1, ROCm, AMD Developer Cloud) as a SIMON node next to
the plant's RTX 3090. Benchmark on the MI300X (Qwen3.8-27B BF16, vLLM): one card sustained 64 concurrent streams;
SIMON added no measurable throughput cost (510/510 receipts verified). In our pre-registered cross-vendor test the
audit flagged the same 5 of 32 prompts on the MI300X as on an RTX 3090 checked against itself, so it does not
penalise a node for its GPU vendor. The codebase spans 5 modular Rust crates (`simon-core`, `simon-verify-wasm`, `simon-harness`, `simon-node`, `simon-cli`) with automated unit/integration tests (run by CI on every push) and Python self-checks.

**Honest limits.** A receipt proves who signed an output, not which model ran (execution audit is research, not
shipped). AI4I is synthetic; its labels follow the same physics as the rules, so rule recall describes the
dataset, not a real plant. A technician reviews every ticket.

## Technology & category tags

AMD Developer Cloud · AMD Instinct MI300X · ROCm · vLLM · Gemma 4 · llama.cpp · Rust · libp2p · Ed25519 ·
WebAssembly · SvelteKit · Predictive maintenance · Manufacturing · Verifiable AI

## Links

- Demo: https://pop-os.tailaf1556.ts.net/simon/industry (also /simon/network, /simon/demo)
- Static mirror (no dependency on our machine): https://szpili.github.io/SIMON/industry
- Code (MIT): https://github.com/Szpili/SIMON
- Slides: docs/zgloszenie/SIMON-slajdy.pdf · Video: docs/zgloszenie/wideo/SIMON-wideo.mp4 (4:32, narrated)
