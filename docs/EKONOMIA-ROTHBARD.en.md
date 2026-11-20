# SIMON — economics derived from Rothbard

**Status: analysis / proposal. Not a decision, not implemented.** 2026-11-20.
Polish: [`EKONOMIA-ROTHBARD.md`](EKONOMIA-ROTHBARD.md).
Related: [`EKONOMIA-STAKING.md`](EKONOMIA-STAKING.md), [`WASH-COMPUTE.md`](WASH-COMPUTE.md), [`THREAT-MODEL.md`](THREAT-MODEL.md).

**Source:** 75 recordings of Murray Rothbard's lectures and audiobooks (54.8 h, 494,769 words), transcribed locally
(whisper large-v3-turbo). The series are *Ethics of Liberty*, *For a New Liberty*, *History of Economic Thought*,
*What Has Government Done to Our Money* (WHGDTOM below), and a set of standalone lectures. Principles were extracted
by 8 LLM workers reading the full transcripts, and chapters 2–3 of WHGDTOM were also read in full by hand.
**Review:** GPT-6.2 Astra returned **CHANGES NEEDED**. This document already includes its corrections.

## The conclusion in one paragraph

Rothbard gives SIMON one hard rule: **income in the network may come only from voluntary payment for a verified
service.** In his terms, issuing tokens to nodes is counterfeiting: it moves wealth to whoever receives the new money
first (the Cantillon effect). This is exactly what sank Golem, Helium and Filecoin, where issuance financed farms
instead of the product. An "on demand" credit with no reserved capacity is fractional reserve, and the morning peak
would be the bank run. **MVP:** a single order priced in PLN, 100% prepaid, priced by the seller, with no token and
no issuance. The night-to-peak exchange is still open. It has to be designed as an honest, collateralised instrument.

## WHY — principles and what they mean for SIMON

| Rothbard's principle (paraphrase) | Where | Consequence for SIMON |
|---|---|---|
| Money cannot be decreed. It grows out of a good already valued for direct use (the regression theorem) | WHGDTOM ch. 2 | The only value that already exists in the network is verified compute, so we price it in PLN. A second asset makes sense only once the market itself starts using it |
| Any money supply is as good as any other. New money creates nothing and only redistributes to the first receivers | WHGDTOM ch. 2–3 | Per-epoch issuance to miners is a transfer from late participants to early ones. Zero inflationary issuance is an explicit design choice |
| A receipt for goods that are not in the warehouse is fraud. A fractional-reserve bank is always insolvent, which becomes visible only in a run | WHGDTOM ch. 2 | Compute is perishable: night GPU-hours cannot be stored. A "spend it in the morning whenever you like" credit with no reservation is a run at 09:00 |
| Gresham's law operates only under a forced par (bimetallism, legal tender) | WHGDTOM ch. 3 | Counting cached prefill the same as computed prefill, or quantized output the same as full precision, makes nodes deliver the "worn coin" |
| A contract is a transfer of title, not a promise. Only breaches that amount to implicit theft are enforceable | *Ethics of Liberty* ch. 19 | The protocol enforces escrow and bonds, not bare promises of availability |
| Punishment is proportional, and restitution comes first | *Ethics* ch. 13 | First a refund to the client, then agreed compensation, and last a capped penalty. Never confiscation "for the protocol" |
| Homesteading: labour creates title, mere "discovery" does not | *Ethics* ch. 6–11 | No premine. Reputation and priority are earned through independently paid work |
| Competing agencies arbitrate, and a dominant agency is not inevitable (contra Nozick) | *Ethics* ch. 29 | The protocol must not forbid competing coordinators or verifiers |
| A boycott is not aggression. Value is subjective, and cost does not set price | *Ethics* ch. 18; *Demand and Supply* | Public blacklists are fine. "3 PLN of electricity vs 2.2 PLN of tokens" is not the verdict. The verdict would be the lack of a paid renewal |

## Review (GPT-6.2 Astra) — what it changed

| Rule in the first draft | Verdict | Reason |
|---|---|---|
| regression theorem, zero issuance, 100% backing, time preference, Gresham, minter's brand, punishment, income from service, non-transferability | **FIX** | The direction holds, but the draft was imprecise. A reservation is a promise of future performance, not title to an existing asset (hence two separate ledgers). Night plus morning is two spot trades. A seller's fixed price with limited inventory is legitimate. A signature proves neither operator independence nor accurate counters |
| no legal tender, subjective value, no demurrage | **KEEP** | — |
| "subsidised GPUs = business cycle" | **DROP** | Austrian business-cycle theory needs credit expansion and an interest-rate mechanism. What exists here is subsidy dependence, which has to be measured |
| ban on coordinator cartels | **DROP** | Voluntary pooling is legitimate. The tests are exclusive control, outstanding liabilities and settlement finality |
| "protocol margin = seigniorage / tax" | **DROP** | A disclosed margin on a voluntary service is not a tax |

A point of precision: Helium fixes the **USD value of a Data Credit**, and the number of DCs per burned HNT varies.
A fixed SIMON→THINK rate, a fixed PLN value for THINK and a fixed THINK tariff for inference are three different
commitments. None of them reserves a GPU.

**Three holes the review found:**
1. **The verifier.** If it is paid only on acceptance, an honest rejection costs it income. Fix: a fixed fee for every
   completed review whatever the verdict, with the verifier assigned independently of the executor.
2. **Double-selling.** Two coordinators can sell the same GPU for 09:00. Fix: one admission ledger per device, an exposure
   cap per operator, and fallback capacity in a different failure domain.
3. **The privacy premium.** It disappears at the verifier, which also sees the prompt. Fix: both the permitted executor
   **and** the verifier are written into the signed order, and the restricted pool is priced separately.

## HOW — MVP spec (after the review)

1. **A single order in PLN.** No token, no burn, no per-epoch issuance, no THINK balance, no accrual for idle hours.
2. **A signed offer:** seller, model and quantization hashes, runtime, verification method, permitted executors and
   verifiers, input and output limits, deadline, maximum price, fee split, failure terms. The provider sets the price.
3. **Reproducible billing:** at the start, a fixed price per order within the signed limits. `C = w_p·P + w_d·D` is
   only a versioned benchmark until it can settle a dispute.
4. **100% funding before dispatch** through an authorised payment provider. Client PLN is reconciled daily against
   safeguarded PLN. New orders are frozen on any shortfall. Operator bonds and sponsor money are kept separate.
5. **Reserve before promising:** a firm deadline exists only with named capacity, a remedy and collateral. Best-effort
   jobs are labelled as such.
6. **Settle once,** bound to a unique order ID. The verifier receives a fixed fee whatever the verdict. Retries are
   idempotent.
7. **Routing independent of volume:** self-reported turnover, job counts and ratings carry zero weight. This is the
   real defence against wash-compute: with zero issuance, better routing would be the only payoff from self-dealing.
8. **Gradual scaling:** 1 order from an unrelated client → an unsubsidised renewal → 10–12 varied cases (rejection,
   timeout, refund, peak contention) → only then hundreds.
9. **OPEN:** the smallest honest night-to-peak instrument (a reservation or a forward on a time window) with 100%
   collateral and zero issuance. Without it SIMON loses its one distinctive value proposition.

## Law (gates, not advice)

An order paid in PLN, recorded in an ordinary database and carrying a non-negotiable receipt, has strong grounds for
falling outside MiCA's crypto-asset definition (Art. 3(1)(5), recital 17). A balance accepted by independent nodes
changes that analysis and also raises an e-money question (Directive 2009/110/EC, Art. 2(2)). **There is no Polish
PIT exemption:** a node run by a private individual earns service income, including income in kind (a right to
compute). These questions are for a lawyer and a tax adviser, not for a model.

## Not verified

- The LLM workers' notes have not been checked line by line. Only WHGDTOM ch. 2–3 was read in full.
- The reviewer checked Rothbard's published texts, not our transcripts.
- The cost figures (3 PLN of electricity per night on a 3090, 2.2 PLN of tokens) come from the earlier roadmap and
  were not re-measured.
