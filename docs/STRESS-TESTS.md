# Stress and edge-case tests: SIMON inside one company

Status 2026-10-16. Rule: every attack is a reproducible test in the repo and one row here
(attack → test → result → fix, or an honest limit). Scale: 1 case, then 10–12 varied cases, then more.

## Threat model (team call, 2026-10-15)

Start with the deployment we sell first: a small company (say five machines) running SIMON inside its
own VPN. That is a narrow attack surface, so we secure it first and only then the open internet.
Inside the VPN we still assume one dishonest participant: an employee, a compromised node or a
compromised gateway. Three questions:

1. **Tampering detection.** Anything that can be changed must leave a mark we detect.
2. **Process compromise.** Someone swaps the input, the output or the settings (model, precision).
3. **Network inside the VPN.** Eavesdropping, and one noisy or malicious peer blocking the others (DoS).

## Results

| # | Attack | Test (in CI) | Result | Fix / limit |
|---|---|---|---|---|
| 1 | Change one character of a signed answer | `crates/simon-core/tests/tamper_matrix.rs`, UI `/demo` | rejected | output digest is signed |
| 2 | Change one signed receipt field | `tamper_matrix.rs` | rejected | Ed25519 over the canonical receipt |
| 3 | Node mints its own key and self-signs any text | `agent.rs::obcy_klucz_odrzucony_gdy_signer_przypiety` | **verified before the fix**: a receipt carries its own key | key pinning: `--expect-signer`, `node_id` in the gateway's `nodes.json`, `simon verify --signer`; UI shows *plant-listed node* / *unknown node*; responses report `signer_pinned` |
| 4 | Replay an old receipt for a new job | `tamper_matrix.rs::replay_nonce_odrzucony`, agent `job_id` + nonce check | rejected | the nonce always comes from the client's own order |
| 5 | Replay the same request to burn GPU time | `node.rs::limit_peera_odrzuca_powtorke_i_nadmiar` | rejected (`powtorzony_job`) | last 4096 (peer, job_id) remembered |
| 6 | One peer floods a node's queue | same test | rejected above the limit (`rate_limit`) | `--rate-per-peer` (default 16/s, 4 s burst); gateway answers 503 + Retry-After |
| 7 | Forged gossip result marks a healthy node *Degraded* | `coord.rs::niepodpisany_lub_obcy_blad_odrzucony`, `coord.rs::obcy_wynik_nie_degraduje_wezla` | rejected | `SignedJobResult` (`SIMON/JOB-RESULT/v1`) signed with the assigned node's Ed25519 key for both successes and failures; unsigned results or wrong-key signatures rejected without state mutation |
| 8 | Node declares a model or precision it does not run | node test `test_wykryj_precyzje_z_nazwy_modelu`; `docs/VERIFIER-PILOT.md` | **not proven by a receipt** | precision is never guessed (`int8`/`int4`/`unknown`); model identity: detection pilot only (behaviour 46/50, quantization separable), not proof |
| 9 | Dry-run audit passed off as a real execution audit | `m3.rs::symulowany_audyt_odrzucony` | rejected | `simulated` marker; CLI and WASM refuse it |
| 10 | NaN / unsorted log-probs in an audit | `m3.rs` tests | rejected / sorted | — |
| 11 | Prompt injection through machine data into a maintenance ticket | `tools/simon-industry/stress_corpus.json` (17 cases), `tickets.py` self-check | first run: 7/12 unsafe tickets passed (Joe) → now rejected or marked *known limit* | steps only from a per-mode catalog, fields equal the rules, free text short and checked, every ticket reviewed by a technician; **limit:** the meaning of the free-text summary |
| 12 | Triage agent picks failure modes, urgency or action the rules did not | `agent_triage.py` self-check (15 rejections) | rejected | proposal must equal the most urgent rule finding; CSV fields cleaned before the model sees them |
| 13 | Shutdown instruction hidden by spacing, dashes, full-width or zero-width characters | same self-check | rejected | NFKC, format characters stripped, dashes as spaces, synonyms |
| 14 | SSRF to private, loopback or IPv4-mapped IPv6 addresses | `crates/simon-core/src/web.rs` tests | rejected | includes CGNAT 100.64/10 (also the tailnet) |
| 15 | Read the node key from the disk of a shared machine | `tozsamosc.rs` | key file created 0600 | — |
| 16 | Eavesdrop on jobs inside the VPN | by construction | encrypted | libp2p Noise on every connection; the gateway listens on 127.0.0.1; the public monitor never carries prompts or answers |
| 17 | Live M1 token chain tampering or >65536 token OOM flood | `crates/simon-cli/tests/e2e_m1_receipt.rs` | rejected | `Receipt::zweryfikuj_m1` verifies token chain against client's own `order.nonce`; `MAX_M1_TOKENS = 65_536` cap strictly enforced on tokenization and verification |
| 18 | Unsigned / unvouched Sybil node registration flood on gossipsub | `coord.rs::sybil_bez_vouchera_odrzucony_w_trybie_voucher` | rejected | `RegisterMsg` signed with node key (`SIMON/NODE-REGISTER/v1`); coordinator in `--require-voucher` mode enforces operator `NodeVoucher` (`SIMON/NODE-VOUCH/v1`) |

## Honest limits (do not claim otherwise)

- **Execution is not proven.** A receipt proves who signed which text and commits to the declared token sequences, not which model weights executed.
- **Quantization substitution:** Same-family quantization substitution is not separable by output alone without independent hardware telemetry or TEE.
- **Open-network economics:** While enterprise VPN admission is secured via signed registration and operator vouchers (row 18), permissionless open-network staking and wash-compute economics remain research topics.
- **Prompt injection:** Capability ceilings and parameter validation reduce the action blast radius; they do not prevent an LLM from reading or following adversarial instructions in user input.

## Next (owner in the team channel)

1. **[DONE in Stage 2]** Sign `JobResult` (failures too) with the node key (`SIMON/JOB-RESULT/v1`); coordinator verifies before any state change (row 7).
2. **[DONE in Stage 2]** Wire live M1: node tokenizes prompt and output via `/tokenize`, sends token IDs with `MAX_M1_TOKENS = 65_536` caps, and produces verified Level 1 receipts (row 17).
3. **[DONE in Stage 2]** Signed node registration (`SIMON/NODE-REGISTER/v1`) with operator vouching (`SIMON/NODE-VOUCH/v1`) and Sybil admission tests (row 18).
4. **[Next Priority]** A prepared target network test harness for an outside tester (automated runner attacking connected nodes and reporting against this table).
