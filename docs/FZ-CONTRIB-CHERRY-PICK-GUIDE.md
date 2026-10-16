# Team Captain Review & Cherry-Pick Guide (`fz-contrib` Stage 2)

**Audience:** Team Captain (`Szpili`) and core reviewers  
**Base Commit in `upstream/fz-contrib`:** `4186b4d` (`upstream/main`: `5854038`)  
**Shared Staging Branch:** `fz-contrib` (`4186b4d` -> Stage 2 feature tracks)  
**Fast Verification Command:** `bash scripts/verify_all_fast.sh`  

---

## 1. Overview & Strategy

Following the Stage 1 merge (`PR #8` / `d108364`) and post-merge review commits (`d2f0d4f`..`53854d2`, `4186b4d`), Stage 2 addresses the three top items on the Team Captain's `docs/STRESS-TESTS.md` (`## Next` section):

1. **Next #1 & #3 (`[FZ-J]`)**: Cryptographically signed `JobResult` (`SignedJobResult`, `SIMON/JOB-RESULT/v1`, covering both successes and failures) and signed `RegisterMsg` (`SIMON/NODE-REGISTER/v1`) with operator voucher (`SIMON/NODE-VOUCH/v1`) to prevent Sybil floods and forged gossipsub cancellations.
2. **Next #2 (`[FZ-H]`)**: Live Level 1 Token Commitment (`WiazaniaM1`) pipeline in `node.rs` and `wspolne.rs`, with strict array size caps (`MAX_M1_TOKENS = 65_536`) on `/tokenize` inputs and M1 verification, backed by an in-process end-to-end integration test (`e2e_m1_receipt.rs`).
3. **Architecture & Verification Gate (`[FZ-G]`, `[FZ-I]`)**: Upstream architectural documentation (`docs/ARCHITECTURE.md`), updated stress-test threat matrix rows 7/17/18 (`docs/STRESS-TESTS.md`), and a zero-GPU CPU/offline verification script (`scripts/verify_all_fast.sh`).

All commits on `fz-contrib` are strictly granular, self-contained, compile cleanly, and pass tests independently. The Captain may either **merge `fz-contrib` in one go** or **cherry-pick individual track commits** into `main` using the menu below.

---

## 2. Fast Verification for Reviewers (One Command, ~35 seconds)

Reviewers can verify the entire workspace and all guardrails locally without needing a GPU:

```bash
bash scripts/verify_all_fast.sh
```

This single command executes:
1. `python3 scripts/wartownik_stalych.py crates`: Scans all Rust sources to ensure no hardcoded throughput divisors or fabricated metrics exist.
2. `cargo test --workspace`: Executes all unit and integration test suites across `simon-core`, `simon-harness`, `simon-node`, `simon-cli`, and `simon-verify-wasm`.
3. `cargo clippy --workspace --all-targets -- -D warnings`: Lints all targets (including integration test files and examples) with zero warnings allowed.
4. Python Selfcheck Suites: Deterministic self-checks for `tools/simon-industry/rules.py`, `tools/simon-industry/tickets.py`, `tools/simon-industry/agent_triage.py`, `tools/simon-industry/export_ui.py`, `tools/simon-api/`, and `tools/simon-monitor/simon_monitor.py`.
5. Captain Guardrail Greps: Automated audit asserting zero leaked local workspace paths and zero banned overclaiming phrases outside explicit threat model documentation.
6. Optional UI Check: Runs `npm --prefix ui run check` if `ui/node_modules` is installed.

---

## 3. Granular Cherry-Pick Menu (by Category)

### Category 1: Signed `JobResult` & Sybil-Resistant Registration (`[FZ-J]`)
*Addresses: `docs/STRESS-TESTS.md` Next #1 & Next #3*  
*Assigned Track: Track J (`fz/track-J-signed-job-result-and-registration`)*

* **Problem Solved**: On gossipsub, unauthenticated peers could broadcast arbitrary `JobResult` messages to falsely mark nodes degraded, or flood the coordinator registry with spoofed nodes.
* **Mechanism**:
  - `JobResult` (`SignedJobResult`) signed with domain tag `SIMON/JOB-RESULT/v1` over `(job_id, node_id, ok, error, receipt_digest)` by the assigned node's Ed25519 key. Both successes and failures are cryptographically verifiable.
  - `RegisterMsg` signed with domain tag `SIMON/NODE-REGISTER/v1` (proof-of-possession of the advertised public key).
  - Optional operator voucher signed with domain tag `SIMON/NODE-VOUCH/v1` by a recognized plant operator key, enforced by `Registry::register_verified(msg, Some(op))`. **Limit:** the live `simon --role coord` passes `None` (no CLI flag for the operator key yet), so on the running coordinator only the node self-signature is enforced.
* **Test Evidence**:
  - `cargo test -p simon-node -p simon-cli` (`niepodpisany_lub_obcy_blad_odrzucony`, `sybil_bez_vouchera_odrzucony_w_trybie_voucher`, `f3_receipt`)
  - Unit tests covering forged failure rejection, unassigned node rejection, and 50-node Sybil flood rejection.

| Commit | Summary | Touched Files | Cherry-Pick Command |
| :--- | :--- | :--- | :--- |
| `2acd635` | `[FZ-J1] feat(node): sign and verify JobResult (including failures) with node Ed25519 key` | `crates/simon-node/src/lib.rs`, `crates/simon-node/tests/f3_receipt.rs` | `git cherry-pick 2acd6358bb87c4f4a2d91394a765010a55767bd2` |
| `5604df5` | `[FZ-J2] feat(node): add signed RegisterMsg proof-of-possession, operator voucher, and Sybil tests` | `crates/simon-node/src/lib.rs` | `git cherry-pick 5604df5d292d5d400926b1d4dc5453460b67b350` |
| `e7e8a50` | `[FZ-J3] fix(coord): require signed JobResult from assigned node before state change or degradation` | `crates/simon-cli/src/coord.rs` | `git cherry-pick e7e8a50deea488ec751fd9957a06297e96f120a5` |

---

### Category 2: Live M1 Token Binding & Context Capping (`[FZ-H]`)
*Addresses: `docs/STRESS-TESTS.md` Next #2*  
*Assigned Track: Track H (`fz/track-H-e2e-integration-and-live-m1-bridge`)*

* **Problem Solved**: In Stage 1, live `simon --role node` emitted Level 0 receipts (`wiazania: None`) even when connected to `/tokenize`. Furthermore, unbounded token arrays from untrusted endpoints posed an allocation vector.
* **Mechanism**:
  - Enforces `MAX_M1_TOKENS = 65_536` on `/tokenize` response parsing (`wspolne::wyciagnij_id_tokenow`) and token commitment generation (`node::policz`).
  - Wires prompt and output tokenization directly into `zbuduj_podpisany_receipt` (`Receipt::z_wiazaniem_m1`) during live inference, and signs `RegisterMsg` on startup.
  - Populates `PromptResponse::{tokenizer_hash, prompt_token_ids, output_token_ids}` so client-side agents independently compute and verify the token commitment against their own `order.nonce`.
  - In-process integration test (`crates/simon-cli/tests/e2e_m1_receipt.rs`) spins up an in-process mock LLM HTTP server and verifies complete Level 1 receipt issuance, tamper rejection, and graceful Level 0 fallback.
* **Test Evidence**:
  - `cargo test -p simon-cli --test e2e_m1_receipt`
  - `cargo test -p simon-harness -p simon-cli`

| Commit | Summary | Touched Files | Cherry-Pick Command |
| :--- | :--- | :--- | :--- |
| `65cadc7` | `[FZ-H1] fix(cli): enforce MAX_M1_TOKENS cap and strict array parsing in wspolne::tokenizuj_tekst` | `crates/simon-cli/src/wspolne.rs` | `git cherry-pick 65cadc7ea16d79fb4c587ba3ec44da9a33680968` |
| `2360454` | `[FZ-H2] feat(node): wire live M1 token commitments with length caps in node::policz` | `crates/simon-cli/src/node.rs` | `git cherry-pick 2360454f7d2c8086450417d385d6c9e4aa2a48ed` |
| `e3296a8` | `[FZ-H3] test(cli): add E2E live M1 receipt generation and tamper verification test suite` | `crates/simon-cli/tests/e2e_m1_receipt.rs` | `git cherry-pick e3296a8b22dc0959353a7ac2dccb9b02597bd0af` |

---

### Category 3: Architecture Documentation & Fast Verification Gate (`[FZ-G]`, `[FZ-I]`)
*Addresses: Team Captain Review Bottleneck & Documentation Parity*  
*Assigned Tracks: Track G (`fz/track-G-architecture-and-docs-overhaul`) & Track I (`fz/track-I-captain-cherry-pick-menu-and-ci`)*

* **Problem Solved**: The system lacked an authoritative end-to-end architecture specification detailing all data flows and claim hierarchies, and lacked a fast (<40s) unified verification runner.
* **Deliverables**:
  - `docs/ARCHITECTURE.md`: Complete Mermaid topologies, data flows for P2P prompt-response, gossipsub coordination, gateway proxying, and M7 capability-bounded industrial triage.
  - `docs/STRESS-TESTS.md`: Updated rows 7, 17, and 18 reflecting implemented mitigations, with clear distinction between verified claims and honest boundary limits.
  - `scripts/verify_all_fast.sh`: Executable bash runner verifying protocol constants, workspace tests, clippy, python self-checks, and guardrail greps in one step.
* **Test Evidence**:
  - `bash scripts/verify_all_fast.sh`

| Commit | Summary | Touched Files | Cherry-Pick Command |
| :--- | :--- | :--- | :--- |
| `5de1c6c` | `[FZ-G1] docs(arch): add docs/ARCHITECTURE.md with Mermaid data flows and claim hierarchy` | `docs/ARCHITECTURE.md` | `git cherry-pick 5de1c6cf61d542429d4d28f6ac1a1d3d73ae25d0` |
| `063d2bf` | `[FZ-G2] docs(stress-tests,threat-model): update stress-test matrix rows 7/17/18 and milestone statuses` | `docs/STRESS-TESTS.md`, `docs/THREAT-MODEL.md`, `ROADMAP.md`, `docs/whitepaper/SIMON-whitepaper-EN.md` | `git cherry-pick 063d2bf6b9f57dd677e2e765b9befe039211e622` |
| `c40ec8c` | `[FZ-I1] feat(scripts): add scripts/verify_all_fast.sh full-stack verification and guardrail runner` | `scripts/verify_all_fast.sh` | `git cherry-pick c40ec8cdae9e7e8a4a642251743c55a1562b1bdd` |
| `87d9d59` | `[FZ-G3] docs(arch,stress-tests,roadmap): state what Stage 2 does not prove` (voucher is library-level, live M1 token ids are re-tokenized, `tokenizer_hash` hashes the model label) | `docs/ARCHITECTURE.md`, `docs/STRESS-TESTS.md`, `ROADMAP.md` | `git cherry-pick 87d9d59600bda7c51473bed84774410c5d38752c` |

---

## 4. Integration Options for Team Captain (`Szpili`)

### Option A: Merge `fz-contrib` Branch Directly (Recommended)
Because all Stage 2 tracks (`G`, `H`, `I`, `J`) have 100% disjoint file scopes and have been integrated cleanly on `fz-contrib`:
```bash
git checkout main
git pull upstream main
git merge --no-ff fz-contrib -m "Merge Stage 2 from fz-contrib (Tracks G, H, I, J)"
bash scripts/verify_all_fast.sh
```

### Option B: Cherry-Pick Selected Subsystems
If you prefer to review and land subsystems incrementally:
```bash
# 1. Verification script & Architecture Docs (zero risk, immediate review benefit)
git cherry-pick c40ec8c 5de1c6c 063d2bf 87d9d59

# 2. Signed JobResult & Sybil Registration (Category 1)
git cherry-pick 2acd635 5604df5 e7e8a50

# 3. Live M1 Token Chain & E2E Test (Category 2)
git cherry-pick 65cadc7 2360454 e3296a8

# Verify everything together:
bash scripts/verify_all_fast.sh
```

---

## 5. Security & Guardrail Compliance Matrix

Every commit in this guide was audited against the mandatory Captain Review Guardrails:
* **Zero Guessed Claims**: Quantization markers strictly resolve to known enums or `Precision::Unknown`. Unbacked proof levels are never assumed.
* **Zero Overclaiming**: Follows `docs/THREAT-MODEL.md` boundary — cryptographic verification proves the signer key and digest of the output, not model weights execution.
* **Deterministic Bounds**: `MAX_M1_TOKENS = 65_536` enforced on all token arrays.
* **Zero Leaked Paths**: Cleaned of any machine-specific or temporary directory paths.
