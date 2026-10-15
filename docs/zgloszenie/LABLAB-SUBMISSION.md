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
2. Gemma 4 26B-A4B drafts the technician ticket as JSON from the rule findings only.
3. Each ticket is checked: valid JSON, failure modes equal to the rules, no number the input does not contain,
   no instruction to stop or bypass equipment, marked for human review.
4. The GPU node signs the exact ticket. The /industry page re-checks every receipt in the browser.

Result on a predeclared, stratified sample: 15 of 15 tickets passed every check, median 1.7 s per ticket, all
receipts valid. Labelled failures the rules miss are shown as misses, not hidden.

**AMD.** Gemma 4 runs on an AMD Instinct MI300X (vLLM 0.27.1, ROCm, AMD Developer Cloud) as a SIMON node next to
the plant's RTX 3090. Measured on the MI300X: one card sustained 64 concurrent streams of a 27B model; SIMON
added no measurable throughput cost (510/510 receipts verified); an honest AMD node flagged exactly the same
prompts as the NVIDIA reference in our pre-registered cross-vendor test.

**Honest limits.** A receipt proves who signed an output, not which model ran (execution audit is research, not
shipped). AI4I is synthetic; its labels follow the same physics as the rules, so rule recall describes the
dataset, not a real plant. A technician reviews every ticket.

## Technology & category tags

AMD Developer Cloud · AMD Instinct MI300X · ROCm · vLLM · Gemma 4 · llama.cpp · Rust · libp2p · Ed25519 ·
WebAssembly · SvelteKit · Predictive maintenance · Manufacturing · Verifiable AI

## Links

- Demo: https://pop-os.tailaf1556.ts.net/simon/industry (also /simon/network, /simon/demo)
- Code (MIT): https://github.com/Szpili/SIMON
- Slides: docs/zgloszenie/SIMON-slajdy.pdf · Video: SIMON-explainer-amd.mp4
