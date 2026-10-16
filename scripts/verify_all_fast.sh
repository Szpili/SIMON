#!/usr/bin/env bash
# ==============================================================================
# SIMON Fast Verification Gate & Team Captain Guardrail Runner
#
# Runs full offline/CPU test suites and Captain review guardrails in < 30-40s:
#   1. Protocol & constant sync guard (scripts/wartownik_stalych.py)
#   2. Rust unit & integration tests across all workspace crates (cargo test --workspace)
#   3. Rust Clippy lints across all targets (cargo clippy --workspace --all-targets -- -D warnings)
#   4. Python tool selfcheck suites (rules, tickets, agent_triage, export_ui, simon-api)
#   5. Captain Guardrail Greps:
#      - Zero workspace path leaks (SIMPN_ARGON_HANDOWER, /home/agent/) in tracked files
#      - Zero banned overclaim phrases outside docs/THREAT-MODEL.md
#   6. Optional UI check: npm run check inside ui/ (if ui/node_modules exists)
# ==============================================================================
set -euo pipefail

# Ensure rustc/cargo and standard tools are in PATH
export PATH="$HOME/.cargo/bin:/home/agent/.cargo/bin:/usr/local/bin:$PATH"

# Resolve repo root directory
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Timing helpers
now_s() {
    python3 -c "import time; print(time.time())"
}

TOTAL_START="$(now_s)"

# Colors if connected to a terminal
if [ -t 1 ]; then
    C_RESET="\033[0m"
    C_BOLD="\033[1m"
    C_GREEN="\033[32m"
    C_RED="\033[31m"
    C_YELLOW="\033[33m"
    C_CYAN="\033[36m"
else
    C_RESET=""
    C_BOLD=""
    C_GREEN=""
    C_RED=""
    C_YELLOW=""
    C_CYAN=""
fi

echo -e "${C_BOLD}${C_CYAN}================================================================================${C_RESET}"
echo -e "${C_BOLD}${C_CYAN} SIMON Fast Verification Gate & Team Captain Guardrail Suite${C_RESET}"
echo -e "${C_BOLD}${C_CYAN}================================================================================${C_RESET}"
echo " Repository root: $REPO_ROOT"
echo " Active branch  : $(git branch --show-current 2>/dev/null || echo 'detached')"
echo " HEAD commit    : $(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
echo ""

STEP_NAMES=()
STEP_STATUSES=()
STEP_DURATIONS=()
ANY_FAILED=0

run_step() {
    local num="$1"
    local name="$2"
    local cmd="$3"

    echo -e "${C_BOLD}--> [Step $num] $name...${C_RESET}"
    local t0
    t0="$(now_s)"
    local status="PASS"
    local output=""

    if output="$(eval "$cmd" 2>&1)"; then
        status="PASS"
        echo -e "    ${C_GREEN}[PASS]${C_RESET} Completed successfully."
    else
        status="FAIL"
        ANY_FAILED=1
        echo -e "    ${C_RED}[FAIL] Command exited with error!${C_RESET}"
        echo "------------------------------------------------------------------------"
        echo "$output"
        echo "------------------------------------------------------------------------"
    fi

    local t1
    t1="$(now_s)"
    local dur
    dur="$(python3 -c "print(f'{$t1 - $t0:.2f}s')")"

    STEP_NAMES+=("$name")
    STEP_STATUSES+=("$status")
    STEP_DURATIONS+=("$dur")
    echo ""
}

# ------------------------------------------------------------------------------
# 1. Constant & Protocol Guard
# ------------------------------------------------------------------------------
run_step 1 "Wartownik Stalych (scripts/wartownik_stalych.py)" \
    "python3 scripts/wartownik_stalych.py crates"

# ------------------------------------------------------------------------------
# 2. Rust Workspace Tests
# ------------------------------------------------------------------------------
run_step 2 "Rust Workspace Tests (cargo test --workspace)" \
    "cargo test --workspace"

# ------------------------------------------------------------------------------
# 3. Rust Clippy Gate across all targets
# ------------------------------------------------------------------------------
run_step 3 "Rust Clippy Gate (cargo clippy --workspace --all-targets -- -D warnings)" \
    "cargo clippy --workspace --all-targets -- -D warnings"

# ------------------------------------------------------------------------------
# 4. Python Selfcheck Suites
# ------------------------------------------------------------------------------
run_step 4 "Python Selfcheck Suites (rules, tickets, agent_triage, export_ui, gateway)" \
    'python3 tools/simon-industry/rules.py && \
     python3 -c "import sys; sys.path.insert(0, '\''tools/simon-industry'\''); import tickets; tickets.selfcheck()" && \
     python3 tools/simon-industry/agent_triage.py --selfcheck && \
     python3 tools/simon-industry/export_ui.py >/dev/null && \
     (if python3 tools/simon-api/simon_api.py --selfcheck >/dev/null 2>&1; then \
          python3 tools/simon-api/simon_api.py --selfcheck; \
      elif [ -f tools/simon-api/test_gateway.py ]; then \
          python3 tools/simon-api/test_gateway.py; \
      else \
          python3 tools/simon-api/test_simon_api.py; \
      fi) && \
     python3 -c "import sys; sys.path.insert(0, '\''tools/simon-monitor'\''); import simon_monitor; simon_monitor.selfcheck()"'

# ------------------------------------------------------------------------------
# 5. Captain Guardrail Greps (Zero Path Leaks & Zero Banned Overclaims)
# ------------------------------------------------------------------------------
guardrail_check() {
    local leaks
    leaks="$(git grep -nE "SIMPN_ARGON_HANDOWER|/home/agent/" -- ':!scripts/verify_all_fast.sh' || true)"
    if [ -n "$leaks" ]; then
        echo "ERROR: Captain Guardrail Violation: Leaked local workspace path in tracked files:"
        echo "$leaks"
        return 1
    fi

    local overclaims
    overclaims="$(git grep -niE "100% Authentic|100% Cryptographically Verified|guarantees the claimed model|SIMULATED P2P TOPOLOGY|ed25519:operator_sig_" -- ':!docs/THREAT-MODEL.md' ':!scripts/verify_all_fast.sh' || true)"
    if [ -n "$overclaims" ]; then
        echo "ERROR: Captain Guardrail Violation: Banned overclaim or fake signature phrase in tracked files:"
        echo "$overclaims"
        return 1
    fi
    return 0
}

run_step 5 "Captain Guardrail Greps (Path Leaks & Banned Overclaims)" \
    "guardrail_check"

# ------------------------------------------------------------------------------
# 6. Optional UI Check (runs only if ui/node_modules exists)
# ------------------------------------------------------------------------------
ui_check() {
    if [ -d "ui/node_modules" ]; then
        npm --prefix ui run check
    else
        echo "ui/node_modules not installed; skipping optional UI check."
    fi
}

run_step 6 "Optional UI Typecheck (npm --prefix ui run check)" \
    "ui_check"

# ------------------------------------------------------------------------------
# Summary Table
# ------------------------------------------------------------------------------
TOTAL_END="$(now_s)"
TOTAL_DUR="$(python3 -c "print(f'{$TOTAL_END - $TOTAL_START:.2f}s')")"

echo -e "${C_BOLD}================================================================================${C_RESET}"
echo -e "${C_BOLD} SIMON Fast Verification Summary Report${C_RESET}"
echo -e "${C_BOLD}================================================================================${C_RESET}"
printf "%-3s | %-56s | %-6s | %-8s\n" "#" "Verification Step" "Status" "Duration"
echo "----+----------------------------------------------------------+--------+---------"

for i in "${!STEP_NAMES[@]}"; do
    num=$((i + 1))
    sname="${STEP_NAMES[$i]}"
    status="${STEP_STATUSES[$i]}"
    dur="${STEP_DURATIONS[$i]}"

    if [ "$status" = "PASS" ]; then
        status_colored="${C_GREEN} PASS ${C_RESET}"
    else
        status_colored="${C_RED} FAIL ${C_RESET}"
    fi

    printf "%-3d | %-56s | %b | %8s\n" "$num" "$sname" "$status_colored" "$dur"
done

echo "----+----------------------------------------------------------+--------+---------"
echo -e " Total Duration: ${C_BOLD}${TOTAL_DUR}${C_RESET}"
echo -e "${C_BOLD}================================================================================${C_RESET}"

if [ "$ANY_FAILED" -ne 0 ]; then
    echo -e "${C_RED}${C_BOLD}FAILED: One or more verification steps failed. Review logs above.${C_RESET}"
    exit 1
fi

echo -e "${C_GREEN}${C_BOLD}SUCCESS: All verification checks and guardrails passed cleanly!${C_RESET}"
exit 0
