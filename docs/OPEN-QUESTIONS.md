# SIMON — Open Questions (for human review)

These are the questions that need human judgement, not another model run. They follow from the
measured state: `docs/THREAT-MODEL.md` (what is proven / empirically detected / not proven) and
the negative substitution result in `ROADMAP.md`.

## Trust & verification
1. Is the repositioning from "verifiable inference **execution**" to "verifiable inference-
   **result publication**" honest enough, or still overclaimed?
2. Is **economic staking + a dispute window + reputation** a real system-level answer, or theater?
   What conditions must hold for it not to be theater (adjudication, evidence rules, Sybil cost)?
3. Do you know a pair of models with an **identical tokenizer** (smaller vs larger) for an honest
   substitution test? (Minitron-7B uses a different tokenizer than Bielik-11B, so that test was
   invalid.)
4. Is a genuinely **independent backend** (Transformers/PyTorch vs llama.cpp) worth trying as
   evidence, or a waste? (Two llama.cpp instances are not independent.)

## Prompt injection
5. Is the shape right: capability ceiling (the LLM fills slots, it does not define actions) +
   an operator grant (crypto-bound, exact action + approved parameter values, TTL, single-use,
   revocable) + deny-by-default information flow? What breaks it in ten minutes?
6. Sandboxing / multi-model filtering / input re-prompting / output allowlists — what actually
   works, and what is marketing?
7. Whoever hosts the guard sees the prompt (RULES #1). How does that square with the
   "local/private" positioning? Never claim the guard gives privacy.

## Protocol / P2P / DoS
8. Attack surfaces: Sybil, eclipse, gossip poisoning, replay, DoS. We have nonce windows, rate
   limiting, message-size bounds and cancellation — what is missing?
9. A node that signs plausible receipts but consumes unbounded CPU/IO is a DoS surface. How would
   you bound it in a P2P setting?

## Law / policy (Snowden thread)
10. State-action doctrine and "operator-owned/edge": is that a real shield or an illusion? What
    must we never promise?
11. The guard sees content (personal data). How should the consortium/lawyer be briefed? (Prompt
    encryption does not defend against injection — already established.)

## Demo / judge (AMD hackathon)
12. What would convince a technical judge in five minutes? We have `simon verify`, a browser WASM
    demo (levels 0/1), `simon guard`, and a **negative result**. Should the negative result be shown?

## People / consortium
13. Who else should be invited, in which role (verifier / auditor / operator / lawyer)? What is the
    minimal consortium to keep this funded?

## Already tried (do not repeat)
- Exact-match token audit across backends — dead (E0: 4/4 divergences from ~3 tokens).
- Top-2 containment — does not catch quantization substitution; honest false-flags at max_poza=0.02.
- Minitron-7B as "smaller same-family" — different tokenizer (invalid test).
- ZKML — 10^4–10^6x cost, out.
