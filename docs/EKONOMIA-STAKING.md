# SIMON — Staking, Dispute & Reputation (design)

**Status: design, not implemented.** This is the economic overlay, **not** a verification
mechanism. It deters and prices some misbehaviour; it does **not** prove that a computation was
honest (see `docs/THREAT-MODEL.md`). It is written to be honest about its failure modes.

## Principle

A receipt proves *authorship and binding*. The economic layer prices **provable protocol
violations**. It must never punish on the basis of an unprovable claim ("your model was
different") or on a diagnostic signal (M3 soft-fail).

## Slashable (deterministic, verifiable without a model)

1. Invalid Ed25519 signature over the receipt.
2. Receipt that fails `verify_for_node` (wrong registered key / job_id / node_id).
3. Replay: a nonce reused, or two conflicting signed receipts for the same nonce/job.
4. Malformed or oversized message (protocol bounds; `simon_core::protokol`).
5. Declared resource limits exceeded and signed as if respected.
6. Refusal/failure to respond within the deadline after accepting an order (liveness).

## Never slashable

1. M3 `SoftFail` / `HardFail` on its own — M3 is a **diagnostic** (quantization substitution was
   not separated; honest runs also show deviations).
2. "The model was substituted" — not provable without a TEE.
3. A single token deviation.
4. A guard/toxicity classifier verdict (it is a heuristic and sees content).

## Flow

```
order accepted -> job runs -> receipt signed & published
      -> dispute window (T) : anyone may challenge
           - challenge must cite a PROVABLE violation (above)
           - evidence = signed artifacts + protocol checks (+ optional independent re-run by an
             adjudicator, for cases we explicitly allow)
      -> if no valid challenge: settle
      -> if valid: slash + reputation penalty (bounded, appealable once)
```

- The **dispute window** is explicit and time-bounded; settlement happens only after it closes.
- A challenge costs a small bond (anti-spam); a frivolous challenge loses the bond.
- Evidence rules are **pre-registered** per violation type; nothing is judged case-by-case by
  vibes.

## Arbitration

- A verifier that earns only by catching cheats stops checking under an honest network
  (*verifier's dilemma*). Fix: **pay for coverage, not only for catches** (random audit stipend).
- Decisions on slashable cases should require a **quorum** (≥3 independent adjudicators for
  high-value cases), chosen at random from a bonded pool, with rotation.
- Adjudicators must have no stake in the specific node; collusion risk is mitigated by random
  assignment, not assumed away.

## Reputation

- Per **operator key** (portable identity), not per machine.
- Penalise only **provable** violations; decay over time.
- Never present reputation as proof of honesty; only as evidence of a track record.

## Sybil / cost

- A **minimum stake** per operator makes mass fake identities costly; it does not make them
  impossible.
- Sybil resistance also needs an identity/settlement cost and bounded registration; state this
  explicitly rather than claiming immunity.

## Known failure modes (state them)

- Verifier's dilemma (covered by the coverage stipend above).
- Collusion node↔verifier (random assignment + quorum + rotation; not eliminated).
- Nothing-at-stake / stake grinding (unbonding delay, slashing on provable violations).
- False accusations: mitigated by restricting slashing to provable violations and by the
  challenge bond.
- Economic deterrence is not detection; a funded attacker may still cheat on unprovable axes.

## Open questions for the consortium

- Minimum stake relative to the value of a job? Coverage stipend size?
- Dispute window length vs demo latency?
- Who is a legal adjudicator (people, not code) for high-value cases?
- Can the stake be a stable unit without triggering securities/law issues (see the lawyer brief)?
