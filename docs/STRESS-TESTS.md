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
| 7 | Forged gossip result marks a healthy node *Degraded* | `coord.rs::obcy_wynik_nie_degraduje_wezla` | **was possible** with one unsigned message | results count only for the job assigned to that node; **limit:** failures are unsigned, so a peer that knows a fresh (job, node) pair can still report it failed. Fix: sign `JobResult` with the node key |
| 8 | Node declares a model or precision it does not run | node test `test_wykryj_precyzje_z_nazwy_modelu`; `docs/VERIFIER-PILOT.md` | **not proven by a receipt** | precision is never guessed (`int8`/`int4`/`unknown`); model identity: detection pilot only (behaviour 46/50, quantization separable), not proof |
| 9 | Dry-run audit passed off as a real execution audit | `m3.rs::symulowany_audyt_odrzucony` | rejected | `simulated` marker; CLI and WASM refuse it |
| 10 | NaN / unsorted log-probs in an audit | `m3.rs` tests | rejected / sorted | — |
| 11 | Prompt injection through machine data into a maintenance ticket | `tools/simon-industry/stress_corpus.json` (17 cases), `tickets.py` self-check | first run: 7/12 unsafe tickets passed (Joe) → now rejected or marked *known limit* | steps only from a per-mode catalog, fields equal the rules, free text short and checked, every ticket reviewed by a technician; **limit:** the meaning of the free-text summary |
| 12 | Triage agent picks failure modes, urgency or action the rules did not | `agent_triage.py` self-check (15 rejections) | rejected | proposal must equal the most urgent rule finding; CSV fields cleaned before the model sees them |
| 13 | Shutdown instruction hidden by spacing, dashes, full-width or zero-width characters | same self-check | rejected | NFKC, format characters stripped, dashes as spaces, synonyms |
| 14 | SSRF to private, loopback or IPv4-mapped IPv6 addresses | `crates/simon-core/src/web.rs` tests | rejected | includes CGNAT 100.64/10 (also the tailnet) |
| 15 | Read the node key from the disk of a shared machine | `tozsamosc.rs` | key file created 0600 | — |
| 16 | Eavesdrop on jobs inside the VPN | by construction | encrypted | libp2p Noise on every connection; the gateway listens on 127.0.0.1; the public monitor never carries prompts or answers |

## Honest limits (do not claim otherwise)

- **Live M1 token binding is not wired.** Verification works offline and in WASM, but the node does not
  yet send token ids, so live proof level stays 0 (and the API says so).
- **Execution is not proven.** A receipt proves who signed which text, not which model ran.
- **Sybil / wash-compute:** node registration on gossip is unsigned; open-network economics are research.
- **Unsigned failure results** (row 7).

## Next (owner in the team channel)

1. Sign `JobResult` (failures too) with the node key; coordinator verifies before any state change.
2. Wire live M1: node tokenizes prompt and output via the backend, sends ids with length caps.
3. Signed node registration (keys and vouching for new nodes), then Sybil tests.
4. A prepared target network for an outside tester (attack the connected nodes, report against this table).
