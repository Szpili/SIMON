# SIMON — council review: economics and development paths

**Status: analysis / recommendation, not a decision.** 2026-11-20.
Polish: [`EKONOMIA-KONSYLIUM.md`](EKONOMIA-KONSYLIUM.md). Follows: [`EKONOMIA-ROTHBARD.en.md`](EKONOMIA-ROTHBARD.en.md).

**Order of voices:** principles from Rothbard (Claude Opus) → review by GPT-6.2 Astra → review by Gemini 4 Neon (and a run of
its simulation) → a council of 7 models (GPT/codex, DeepSeek ×2, Gemini, cline-free, Cursor acting as a Polish
payment-regulation (KNF) expert, GPT-5.6 Sol) → Argon (Gemini, separate answer).

## Conclusion

**No token, no escrow, no balances, no bonds. For 30 days we sell only a paid local-inference benchmark (F) on data with no
personal data, at no less than 2000 PLN net, on an ordinary invoice. The sales conversations decide whether product A1 or A2
exists. Network code is frozen for 30 days.** Two sceptics (both DeepSeeks) declined to pick a path because there is no demand
data at all. That is an honest conclusion: every path is a hypothesis today.

## What was refuted

| Claim | Refuted by | Why |
|---|---|---|
| "100% prepayment in PLN through a payment provider" settles the money question (Opus, Astra, Neon) | KNF expert, GPT-5.6 Sol, codex | Escrow, bonds and release on a verdict are in practice a payment service (Art. 3 of the Polish Payment Services Act), which points to a payment-institution licence (MIP/KIP). A prepaid balance in a database also means holding other people's money. Safer variants, **for a lawyer**: SIMON as merchant of record paying operators on B2B invoices, each operator as its own merchant, or a marketplace under a licensed provider |
| A window forward with a bond κ ≥ 1.5 deters double-selling (Neon) | KNF + calculation | Multi-party bonds mean managing other people's money. Besides, at low demand a bond does not deter (table below) |
| One admission ledger per device blocks double-selling | codex, GPT-5.6 Sol | Without Sybil-resistant hardware attestation, an operator registers the same card under several keys or sells it outside SIMON |
| A trusted pool under NDA provides privacy (Neon) | codex, GPT-5.6 Sol, DeepSeek | An NDA gives legal recourse, not technical confidentiality. The operator becomes a GDPR processor (DPA, retention, logs) |
| Path D: verifiable inference | GPT-5.6 Sol, codex | Today this is a false marketing claim. Output is "signed by the operator", not "verified", until TopLoc false-accept and false-reject rates are measured across GPUs |
| Canary jobs solve the verifier problem (Neon) | GPT-5.6 Sol, DeepSeek | A verifier can merely sample. Without a measured cost of full verification there is no proof that cheating has negative expected value |
| Consistency with Rothbard as an argument for a path (Opus) | DeepSeek ×2, GPT-5.6 Sol | A normative principle is not market feasibility. Zero issuance does not create demand |

**What survives from Rothbard** is what the council confirmed on regulatory and operational grounds: no token (unanimous),
income only from payment for a service, and no premine.

### Double-selling — the correct calculation

The first analysis of Neon's simulation used the wrong metric, the p95 of a single draw (Argon pointed this out). The correct
comparison is the expected value of a cheater who lists one slot with 3 coordinators against an honest operator listing with
one. The model assumes collisions are always detected.

| Demand p | Honest | κ = 0.5 | κ = 1.0 | κ = 1.5 | κ = 2.5 |
|---|---|---|---|---|---|
| 0.2 | 0.170 | **0.359** | **0.303** | **0.247** | 0.135 |
| 0.35 | 0.297 | **0.454** | 0.292 | 0.130 | −0.195 |
| 0.5 | 0.425 | **0.431** | 0.119 | −0.194 | −0.819 |

Bold: double-selling pays. At low demand, which is what a young network has, it pays even at κ = 1.5.

## Path ranking (GPT-5.6 Sol; denominator: 100 similar attempts)

| # | Path | P(≥ 1000 PLN / 90 days) | Code to first sale |
|---|---|---|---|
| 1 | A. Trusted pool as a paid B2B pilot | 35/100 | 2–4 weeks |
| 2 | F. Paid private-inference benchmark/audit | 30/100 | 1–2 weeks |
| 3 | C. Hosting one niche model for a specific client | 18/100 | 3–6 weeks |
| 4 | E. Research / open source | 10/100 | little |
| 5 | D. Verifiable inference | 5/100 | 2–6+ months |
| 6 | B. Public slot marketplace | 3/100 | 3–6+ months |

This is one model's expert estimate, not data.

## Argon — correction to plan A + F

Verdict: **the council's plan needs correcting.** YES to dropping the token, escrow and code-before-sales. **VETO on path A as
"home GPUs + GDPR/DPA".**

1. **For a client with GDPR requirements, home GPUs are a liability.** A CTO or DPO will choose a dedicated EU GPU server with
   ISO 27001 and one DPA, an EU cloud with zero retention, or their own box, not a chain of sub-processors in private flats.
   Split A into:
   - **A1:** sensitive data on the client's premises or one EU data centre, with SIMON as the audit/gateway layer (signed
     receipts, failover). Honestly, this is no longer a home-GPU network but software or a service.
   - **A2:** 1–3 known home GPUs **only for data without personal data** (agents, batch jobs, evaluations, niche models).
2. **A 1000–2500 PLN pilot with a DPA has negative margin** (a 25–40 h funnel and a 1500–3000 PLN legal review of the DPA;
   all assumptions). Note: at 40 h and 2000 PLN even F comes out at 2 h per 100 PLN, right on the kill threshold.
3. **F validates consulting, not SIMON** (a false positive: "thanks, we'll run it ourselves"). So every conversation must ask
   who will maintain it afterwards and for how much, and test willingness to pay for a signed execution trace ("+20% for a
   receipt with the model hash, or is an HTTP log enough?").
4. **F without personal data.** Caveat: data "anonymised by the client" is often only pseudonymised, and pseudonymised data is
   still personal data. Public or synthetic data only is safer. **For a lawyer.**

## The 30-day plan (after all voices)

1. **Product:** a local-inference benchmark (2–3 open models vs the client's current API) on public or synthetic data,
   ≥ 2000 PLN net, invoice, paid directly to the seller. Every answer carries an Ed25519 receipt (weights hash, quantization,
   runtime, TTFT, tok/s), described as a "signed execution trace", never "verified".
2. **Funnel:** 30 contacts, 15 with a privacy/compliance pain (tests A1) and 15 with a burst/niche-model pain (tests A2).
3. **Day-14 gate:** fewer than 5 booked conversations out of 30 contacts → change the message or channel, not the code.
4. **Day-30 gate:** 0 paid F engagements → STOP the commercial track and keep only path E. F sells but 0/5 want recurring
   maintenance → this is a consulting firm, not a network product.
5. **Code freeze:** the only new code is the benchmark runner (metrics, receipts, hard token limits against OOM).

## YAGNI for this stage

Public order book, VRF, bonds, canary jobs, forwards, escrow, multiple coordinators, reputation routing, S/M/L buckets (before a
distribution of ~1000 real orders exists), wash-compute simulations, token and MiCA work.

## Not verified

The ranking and the costs (funnel, lawyer, DC prices) are model estimates and assumptions, not data. Payment, GDPR and tax
questions need a lawyer. The double-selling table is exact for Neon's model (3 coordinators, collisions always detected), not
for a real market.
