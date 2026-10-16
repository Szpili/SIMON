#!/usr/bin/env python3
"""SIMON Autonomous Industrial Triage Agent with M7 Capability Grants (Zezwolenie).

Implements an autonomous 2-to-4 turn diagnostic agent over the SIMON OpenAI-compatible
gateway (:18600/v1/chat/completions) using Gemma 4 (gemma-4-26b-a4b-it-bf16 on AMD Instinct
MI300X ROCm and gemma-4-26b-a4b-it-qat-q4_0 on NVIDIA RTX 3090).

Key Architectural Guarantees:
  1. Diagnostic Tools (Read-Only):
     - lookup_machine_history(product_type, udi): returns wear/temp trend from ai4i2020.csv
     - check_spare_parts_inventory(failure_mode): returns part IDs, stock level, lead time
     - inspect_sensor_telemetry(udi): returns telemetry and deterministic rule findings
  2. Gated Action Tool & Deterministic M7 Capability Ceiling:
     - propose_work_order(urgency, failure_modes, action_code, part_id, summary):
       Enforces Sufit capability bounds (enum bounds, no ambient authority, no equipment shutdown commands).
       Computes the exact SIMON/GRANT-PARAMS/v1 SHA-256 parameter digest matching
       crates/simon-core/src/cap.rs: odcisk_parametrow_akcji("propose_work_order", args).
       No ERP/PLC execution occurs without a cryptographic Ed25519 operator Zezwolenie grant.
  3. Offline Selfcheck:
     - python3 agent_triage.py --selfcheck (or run with no args) runs full deterministic test suite.
"""
import argparse
import csv
import hashlib
import json
import math
import os
import re
import sys
import time
import unicodedata
import urllib.request
from pathlib import Path

# Try relative or absolute import of rules.py
try:
    import rules
except ImportError:
    from tools.simon_industry import rules

DOMENA_GRANT_PARAMS = "SIMON/GRANT-PARAMS/v1"

# Tylko pilności, które emituje rules.findings() — model nie wymyśla własnych.
ALLOWED_URGENCY = {
    "inspect before next shift",
    "plan tool change",
}

ALLOWED_ACTIONS = {
    "INSPECT_COOLING",
    "CALIBRATE_POWER_DRIVE",
    "REDUCE_FEED_STRAIN",
    "REPLACE_TOOL_INSERT",
}

ALLOWED_MODES = {"HDF", "PWF", "OSF", "TWF"}

REQUIRED_WORK_ORDER_FIELDS = {
    "urgency",
    "failure_modes",
    "action_code",
    "part_id",
    "summary",
}

PARTS_INVENTORY = {
    "HDF": {
        "part_id": "FAN-CLR-8020",
        "name": "Spindle cooling fan & filter cartridge",
        "stock": 14,
        "lead_time_days": 1,
        "action_code": "INSPECT_COOLING",
    },
    "PWF": {
        "part_id": "DRV-INV-440",
        "name": "Siemens Sinamics 440 inverter spindle drive",
        "stock": 3,
        "lead_time_days": 3,
        "action_code": "CALIBRATE_POWER_DRIVE",
    },
    "OSF": {
        "part_id": "CPL-TRQ-250",
        "name": "High-torque elastomeric jaw coupling & spindle bearings",
        "stock": 5,
        "lead_time_days": 2,
        "action_code": "REDUCE_FEED_STRAIN",
    },
    "TWF": {
        "part_id": "INS-CRB-100",
        "name": "TiN coated carbide milling insert set (ISO CNMG)",
        "stock": 48,
        "lead_time_days": 0,
        "action_code": "REPLACE_TOOL_INSERT",
    },
}

FORBIDDEN = re.compile(
    r"\b(shut\s*down|stop\s+the\s+machine|switch\s+off|turn\s+off|power\s+(?:down|off)|cut\s+(?:the\s+)?power"
    r"|de\s*energi[sz]\w*|bypass\w*|override\w*|disabl\w*)\b",
    re.I,
)


def _znormalizuj(t: str) -> str:
    """NFKC (pełna szerokość, ligatury), bez znaków formatujących (zero-width, miękki dywiz),
    myślniki jak spacje, zwinięte białe znaki: 'shut  down', 'shut-down', 'shut\u200bdown' nie przejdą."""
    t = unicodedata.normalize("NFKC", t)
    t = "".join(c for c in t if unicodedata.category(c) != "Cf")
    t = re.sub(r"[-\u2010-\u2015_]", " ", t)
    return re.sub(r"\s+", " ", t)


def _czysty_id(v, dozwolone: str = r"[^A-Za-z0-9-]", maks: int = 16) -> str:
    """Pola z CSV (Product ID, Type) przed modelem: tylko znaki identyfikatora — dane nie niosą instrukcji."""
    return re.sub(dozwolone, "", str(v))[:maks]

TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "lookup_machine_history",
            "description": "Lookup historical wear and temperature trends from the machine fleet telemetry around a specific UDI.",
            "parameters": {
                "type": "object",
                "properties": {
                    "product_type": {
                        "type": "string",
                        "enum": ["L", "M", "H"],
                        "description": "Product quality variant (L, M, or H).",
                    },
                    "udi": {
                        "type": "integer",
                        "description": "Unique Device Identifier (UDI) of the machine.",
                    },
                },
                "required": ["product_type", "udi"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "check_spare_parts_inventory",
            "description": "Check plant inventory stock level, lead time, and replacement part IDs for a diagnosed failure mode.",
            "parameters": {
                "type": "object",
                "properties": {
                    "failure_mode": {
                        "type": "string",
                        "enum": ["HDF", "PWF", "OSF", "TWF"],
                        "description": "Failure mode code (HDF=cooling/filter, PWF=drive/inverter, OSF=torque coupling/bearings, TWF=tool insert).",
                    }
                },
                "required": ["failure_mode"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "inspect_sensor_telemetry",
            "description": "Fetch detailed raw sensor telemetry and physics-based rule findings for a machine record.",
            "parameters": {
                "type": "object",
                "properties": {
                    "udi": {
                        "type": "integer",
                        "description": "Unique Device Identifier (UDI) of the machine record.",
                    }
                },
                "required": ["udi"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "propose_work_order",
            "description": "Propose a gated industrial work order action. Bound by M7 capability ceiling; generates SIMON/GRANT-PARAMS/v1 parameter commitment.",
            "parameters": {
                "type": "object",
                "properties": {
                    "urgency": {
                        "type": "string",
                        "enum": [
                            "inspect before next shift",
                            "plan tool change",
                        ],
                        "description": "Urgency rating verified against plant rules.",
                    },
                    "failure_modes": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "enum": ["HDF", "PWF", "OSF", "TWF"],
                        },
                        "description": "List of confirmed failure modes.",
                    },
                    "action_code": {
                        "type": "string",
                        "enum": [
                            "INSPECT_COOLING",
                            "CALIBRATE_POWER_DRIVE",
                            "REDUCE_FEED_STRAIN",
                            "REPLACE_TOOL_INSERT",
                        ],
                        "description": "Permitted maintenance action code from plant capability ceiling.",
                    },
                    "part_id": {
                        "type": "string",
                        "description": "Inventory part ID required for maintenance.",
                    },
                    "summary": {
                        "type": "string",
                        "description": "Concise summary for technician (max 300 characters). Must not contain shutdown instructions.",
                    },
                },
                "required": [
                    "urgency",
                    "failure_modes",
                    "action_code",
                    "part_id",
                    "summary",
                ],
            },
        },
    },
]

SYSTEM_PROMPT = (
    "You are an autonomous industrial triage agent for a precision machining plant. "
    "Your objective is to diagnose machine anomalies using the available diagnostic tools, "
    "verify spare parts availability, and propose a maintenance work order via propose_work_order. "
    "Never instruct anyone to shut down, switch off, or bypass equipment. "
    "All action proposals are gated by the plant M7 capability ceiling and require cryptographic operator Zezwolenie."
)


def compute_grant_params_digest(akcja: str, args: dict) -> str:
    """Computes exact SIMON/GRANT-PARAMS/v1 commitment digest matching simon-core::cap::odcisk_parametrow_akcji."""
    canon = json.dumps(
        {"akcja": akcja, "domena": DOMENA_GRANT_PARAMS, "parametry": args},
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(b"SIMON/GRANT-PARAMS/v1\x00" + canon).hexdigest()


def find_default_csv() -> Path:
    candidates = [
        Path(__file__).resolve().parent / "data" / "ai4i2020.csv",
        Path("tools/simon-industry/data/ai4i2020.csv"),
    ]
    for c in candidates:
        if c.exists():
            return c
    return candidates[0]


def execute_lookup_machine_history(csv_path: Path, product_type: str, udi: int) -> dict:
    if not csv_path.exists():
        return {"error": f"CSV dataset not found at {csv_path}"}
    rows = list(csv.DictReader(open(csv_path, encoding="utf-8-sig")))
    target_idx = None
    for i, r in enumerate(rows):
        uid_val = r.get("UDI") or r.get("\ufeffUDI") or r.get("udi")
        if uid_val and int(uid_val) == udi:
            target_idx = i
            break
    if target_idx is None:
        return {"error": f"UDI {udi} not found in machine telemetry dataset"}

    start = max(0, target_idx - 3)
    end = min(len(rows), target_idx + 4)
    slice_rows = rows[start:end]

    history = []
    for r in slice_rows:
        cur_uid = int(r.get("UDI") or r.get("\ufeffUDI") or r.get("udi"))
        f = rules.features(r)
        history.append({
            "udi": cur_uid,
            "type": _czysty_id(r["Type"], r"[^A-Za-z]", 2),
            "wear_min": f["wear_min"],
            "temp_diff_K": f["temp_diff_K"],
            "torque_Nm": f["torque_Nm"],
            "rpm": f["rpm"],
            "power_W": f["power_W"],
        })

    target_feat = rules.features(rows[target_idx])
    wear_trend = "increasing" if len(history) > 1 and history[-1]["wear_min"] >= history[0]["wear_min"] else "steady"
    avg_temp_diff = round(sum(h["temp_diff_K"] for h in history) / len(history), 2)

    return {
        "udi": udi,
        "product_type": product_type,
        "sample_window_size": len(history),
        "wear_trend": wear_trend,
        "average_temp_diff_K": avg_temp_diff,
        "recent_readings": history,
        "target_machine": target_feat,
    }


def execute_check_spare_parts(failure_mode: str) -> dict:
    mode = failure_mode.strip().upper()
    if mode not in PARTS_INVENTORY:
        return {
            "error": f"Unknown failure mode {mode}. Expected one of {sorted(PARTS_INVENTORY.keys())}",
            "available_modes": sorted(PARTS_INVENTORY.keys()),
        }
    item = PARTS_INVENTORY[mode]
    return {
        "failure_mode": mode,
        "part_id": item["part_id"],
        "part_name": item["name"],
        "stock_quantity": item["stock"],
        "lead_time_days": item["lead_time_days"],
        "action_code": item["action_code"],
        "in_stock": item["stock"] > 0,
    }


def execute_inspect_sensor_telemetry(csv_path: Path, udi: int) -> dict:
    if not csv_path.exists():
        return {"error": f"CSV dataset not found at {csv_path}"}
    rows = list(csv.DictReader(open(csv_path, encoding="utf-8-sig")))
    for r in rows:
        uid_val = r.get("UDI") or r.get("\ufeffUDI") or r.get("udi")
        if uid_val and int(uid_val) == udi:
            f = rules.features(r)
            finds = rules.findings(f)
            return {
                "udi": udi,
                "product_id": _czysty_id(r["Product ID"]),
                "type": _czysty_id(r["Type"], r"[^A-Za-z]", 2),
                "features": f,
                "findings": finds,
                "allowed_checks": {k: v for x in finds for k, v in rules.CHECKS.get(x["mode"], {}).items()},
            }
    return {"error": f"UDI {udi} not found"}


def execute_propose_work_order(args: dict, findings: list | None) -> dict:
    """Enforces M7 capability ceiling and computes SIMON/GRANT-PARAMS/v1 parameter commitment.

    `findings` = rules.findings() for THIS machine. The rules decide the failure modes, the urgency and
    the action/part per mode; the model may only restate them (plus a short checked summary).
    """
    if not findings:
        raise ValueError("M7 Capability Ceiling Violation: the rules found no risk for this machine; no work order")
    # 1. Enforce strict parameter bounds
    actual_keys = set(args.keys())
    if actual_keys != REQUIRED_WORK_ORDER_FIELDS:
        missing = sorted(REQUIRED_WORK_ORDER_FIELDS - actual_keys)
        unexpected = sorted(actual_keys - REQUIRED_WORK_ORDER_FIELDS)
        raise ValueError(
            f"M7 Capability Ceiling Violation: schema mismatch. Missing: {missing}, Unexpected: {unexpected}"
        )

    urgency = args.get("urgency")
    if urgency not in ALLOWED_URGENCY:
        raise ValueError(
            f"M7 Capability Ceiling Violation: invalid urgency '{urgency}'. Must be in {sorted(ALLOWED_URGENCY)}"
        )

    action_code = args.get("action_code")
    if action_code not in ALLOWED_ACTIONS:
        raise ValueError(
            f"M7 Capability Ceiling Violation: invalid action_code '{action_code}'. Must be in {sorted(ALLOWED_ACTIONS)}"
        )

    failure_modes = args.get("failure_modes")
    if not isinstance(failure_modes, list) or not failure_modes:
        raise ValueError("M7 Capability Ceiling Violation: failure_modes must be a non-empty list")
    for m in failure_modes:
        if m not in ALLOWED_MODES:
            raise ValueError(f"M7 Capability Ceiling Violation: unknown failure mode '{m}'")

    summary = args.get("summary", "")
    if not isinstance(summary, str) or not summary.strip():
        raise ValueError("M7 Capability Ceiling Violation: summary must be non-empty text")
    if len(_znormalizuj(summary)) > 300 or len(summary) > 300:
        raise ValueError(f"M7 Capability Ceiling Violation: summary exceeds 300 characters ({len(summary)})")
    if FORBIDDEN.search(_znormalizuj(summary)):
        matched = FORBIDDEN.search(_znormalizuj(summary)).group(0)
        raise PermissionError(
            f"M7 Capability Ceiling Violation: forbidden shutdown/override command detected: '{matched}'"
        )

    part_id = args.get("part_id")
    known_parts = {v["part_id"] for v in PARTS_INVENTORY.values()}
    if part_id not in known_parts:
        raise ValueError(f"M7 Capability Ceiling Violation: part_id '{part_id}' not found in inventory catalog")

    # Reguły decydują, model tylko powtarza (kontrola krzyżowa z rules.findings dla tej maszyny).
    tryby_reguly = {f["mode"] for f in findings}
    if len(set(failure_modes)) != len(failure_modes):
        raise ValueError("M7 Capability Ceiling Violation: duplicate failure_modes")
    if set(failure_modes) != tryby_reguly:
        raise ValueError(
            f"M7 Capability Ceiling Violation: failure_modes {sorted(set(failure_modes))} != rules {sorted(tryby_reguly)}"
        )
    if urgency != findings[0]["urgency"]:  # findings są posortowane: najpilniejsze pierwsze
        raise ValueError(f"M7 Capability Ceiling Violation: urgency '{urgency}' != rules '{findings[0]['urgency']}'")
    # Zlecenie dotyczy NAJPILNIEJSZEGO ustalenia (rules.findings: najpilniejsze pierwsze): jego pilność i jego akcja.
    glowny = PARTS_INVENTORY.get(findings[0]["mode"])
    if glowny is None:
        raise ValueError(f"M7 Capability Ceiling Violation: no catalog action for rule mode {findings[0]['mode']}")
    if (action_code, part_id) != (glowny["action_code"], glowny["part_id"]):
        raise ValueError(
            f"M7 Capability Ceiling Violation: ({action_code}, {part_id}) is not the catalog action for {findings[0]['mode']}"
        )

    # 2. Compute exact SIMON/GRANT-PARAMS/v1 parameter commitment digest
    parametry_digest = compute_grant_params_digest("propose_work_order", args)

    return {
        "status": "GATED_BY_OPERATOR_CAPABILITY_CEILING",
        "akcja": "propose_work_order",
        "domena": DOMENA_GRANT_PARAMS,
        "parametry_digest": parametry_digest,
        "requires_grant": "Zezwolenie",
        "parameters": args,
        "message": (
            f"Action validated against Sufit ceiling. Direct ERP/PLC execution blocked. "
            f"Awaiting M7 Ed25519 single-use operator Zezwolenie token with digest {parametry_digest[:16]}..."
        ),
    }


def execute_tool(name: str, args: dict, csv_path: Path, findings: list | None = None) -> dict:
    if name == "lookup_machine_history":
        return execute_lookup_machine_history(csv_path, args.get("product_type", "M"), int(args.get("udi", 0)))
    elif name == "check_spare_parts_inventory":
        return execute_check_spare_parts(args.get("failure_mode", ""))
    elif name == "inspect_sensor_telemetry":
        return execute_inspect_sensor_telemetry(csv_path, int(args.get("udi", 0)))
    elif name == "propose_work_order":
        return execute_propose_work_order(args, findings)
    else:
        return {"error": f"Unknown tool '{name}'"}


def run_triage_agent(
    gateway: str,
    model: str,
    udi: int,
    csv_path: Path,
    max_turns: int = 4,
    timeout: int = 300,
) -> dict:
    """Runs autonomous multi-turn tool-calling loop over SIMON gateway."""
    telemetry = execute_inspect_sensor_telemetry(csv_path, udi)
    if "error" in telemetry:
        raise RuntimeError(telemetry["error"])

    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {
            "role": "user",
            "content": (
                # pola z CSV do promptu tylko po oczyszczeniu: dane nie mogą nieść instrukcji
                f"Machine record UDI={udi} (Product {re.sub(r'[^A-Za-z0-9-]', '', str(telemetry.get('product_id')))[:16]}, "
                f"Type {re.sub(r'[^A-Za-z]', '', str(telemetry.get('type')))[:2]}) "
                f"triggered the following deterministic rule findings:\n"
                f"{json.dumps(telemetry.get('findings'), indent=2)}\n\n"
                f"Sensor features:\n"
                f"{json.dumps(telemetry.get('features'), indent=2)}\n\n"
                "Please perform autonomous diagnostic triage: check machine history trend, verify spare parts inventory, "
                "and propose a work order via propose_work_order under M7 capability ceiling constraints."
            ),
        },
    ]

    history = []
    gated_proposals = []
    receipts = []

    for turn in range(1, max_turns + 1):
        payload = {
            "model": model,
            "messages": messages,
            "tools": TOOLS,
            "max_tokens": 400,
            "temperature": 0.0,
        }
        body = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            gateway.rstrip("/") + "/v1/chat/completions",
            data=body,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read().decode("utf-8"))

        choice = data["choices"][0]
        msg = choice["message"]
        messages.append(msg)
        tool_calls = msg.get("tool_calls") or []
        simon_meta = data.get("simon") or {}
        if "receipt" in simon_meta:
            receipts.append(simon_meta["receipt"])

        history.append({
            "turn": turn,
            "finish_reason": choice.get("finish_reason"),
            "tool_calls": len(tool_calls),
            "simon": simon_meta,
        })

        if not tool_calls:
            # Model finished reasoning and returned final text
            break

        # Execute each tool call and append tool responses
        for tc in tool_calls:
            fn = tc["function"]["name"]
            try:
                fn_args = json.loads(tc["function"]["arguments"])
            except Exception:
                fn_args = {}
            try:
                tool_output = execute_tool(fn, fn_args, csv_path, telemetry.get("findings"))
            except (ValueError, PermissionError) as e:
                # odrzucona propozycja wraca do modelu jako wynik narzędzia (ticket nie powstaje), nie wywraca procesu
                tool_output = {"status": "REJECTED_BY_CEILING", "error": str(e)}
            if fn == "propose_work_order" and tool_output.get("status") == "GATED_BY_OPERATOR_CAPABILITY_CEILING":
                gated_proposals.append(tool_output)
            messages.append({
                "role": "tool",
                "tool_call_id": tc["id"],
                "content": json.dumps(tool_output),
            })

    return {
        "udi": udi,
        "turns": len(history),
        "history": history,
        "gated_proposals": gated_proposals,
        "receipts": receipts,
        "messages": messages,
    }


def selfcheck():
    """Deterministic offline verification suite testing M7 capability ceiling & tool loop."""
    print("[agent_triage] Running deterministic self-checks...")
    csv_path = find_default_csv()
    assert csv_path.exists(), f"ai4i2020.csv missing at {csv_path}"

    # 1. Test lookup_machine_history
    hist = execute_lookup_machine_history(csv_path, "M", 4362)
    assert "sample_window_size" in hist and hist["sample_window_size"] > 0, hist
    assert "average_temp_diff_K" in hist
    print("  [Pass 1/5] lookup_machine_history OK")

    # 2. Test check_spare_parts_inventory for all 4 failure modes
    for mode in ["HDF", "PWF", "OSF", "TWF"]:
        part = execute_check_spare_parts(mode)
        assert part["in_stock"] is True and part["part_id"], part
        assert part["action_code"] in ALLOWED_ACTIONS
    print("  [Pass 2/5] check_spare_parts_inventory OK")

    # 3. Test inspect_sensor_telemetry
    tel = execute_inspect_sensor_telemetry(csv_path, 4362)
    assert tel["udi"] == 4362 and "findings" in tel and "features" in tel
    print("  [Pass 3/5] inspect_sensor_telemetry OK")

    # 4. Test propose_work_order valid case & SIMON/GRANT-PARAMS/v1 SHA-256 digest
    valid_args = {
        "urgency": "inspect before next shift",
        "failure_modes": ["HDF"],
        "action_code": "INSPECT_COOLING",
        "part_id": "FAN-CLR-8020",
        "summary": "Inspect spindle air cooling fans, ducts, and filters before next shift.",
    }
    reg = tel["findings"]
    res = execute_propose_work_order(valid_args, reg)
    assert res["status"] == "GATED_BY_OPERATOR_CAPABILITY_CEILING"
    assert res["domena"] == DOMENA_GRANT_PARAMS
    assert res["requires_grant"] == "Zezwolenie"
    expected_canon = json.dumps(
        {"akcja": "propose_work_order", "domena": DOMENA_GRANT_PARAMS, "parametry": valid_args},
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    expected_digest = hashlib.sha256(b"SIMON/GRANT-PARAMS/v1\x00" + expected_canon).hexdigest()
    assert res["parametry_digest"] == expected_digest, f"Digest mismatch: {res['parametry_digest']} vs {expected_digest}"
    print(f"  [Pass 4/5] propose_work_order M7 ceiling & digest OK ({expected_digest[:16]}...)")

    # 5. Test M7 ceiling rejections of invalid/dangerous proposals
    rejected = 0
    # 5a. Bad urgency
    try:
        execute_propose_work_order({**valid_args, "urgency": "immediate emergency"}, reg)
    except ValueError:
        rejected += 1

    # 5b. Bad action code
    try:
        execute_propose_work_order({**valid_args, "action_code": "OVERRIDE_THERMAL_CUTOFF"}, reg)
    except ValueError:
        rejected += 1

    # 5c. Forbidden shutdown command in free text
    try:
        execute_propose_work_order({**valid_args, "summary": "Immediately shut down the machine and disable safety."}, reg)
    except PermissionError:
        rejected += 1

    # 5d. Schema pollution (unlisted parameter / ambient authority)
    try:
        execute_propose_work_order({**valid_args, "ambient_authority": True}, reg)
    except ValueError:
        rejected += 1

    # 5e-5j. Reguły decydują: model nie może zmienić trybu, pilności ani akcji; brak ustaleń = brak zlecenia;
    #        filtr wolnego tekstu odporny na spacje, pełną szerokość i synonimy wyłączenia.
    for zle, wyjatek in [
        ({**valid_args, "failure_modes": ["HDF", "PWF"]}, ValueError),
        ({**valid_args, "urgency": "plan tool change"}, ValueError),
        ({**valid_args, "action_code": "REPLACE_TOOL_INSERT", "part_id": "INS-CRB-100"}, ValueError),
        ({**valid_args, "summary": "Shut  down the spindle first."}, PermissionError),
        ({**valid_args, "summary": "Then ｐｏｗｅｒ off the drive."}, PermissionError),
        ({**valid_args, "summary": "De-energize the cabinet."}, PermissionError),
        ({**valid_args, "summary": "Shut-down and de-energising first."}, PermissionError),
        ({**valid_args, "summary": "shut\u200bdown now"}, PermissionError),
        ({**valid_args, "failure_modes": ["HDF", "HDF"]}, ValueError),
    ]:
        try:
            execute_propose_work_order(zle, reg)
        except wyjatek:
            rejected += 1
    try:
        execute_propose_work_order(valid_args, [])
    except ValueError:
        rejected += 1

    # dwa ustalenia (HDF najpilniejsze, potem TWF): akcja/część TWF z pilnością HDF to podmiana — odrzucone
    dwa = [{"mode": "HDF", "urgency": "inspect before next shift"}, {"mode": "TWF", "urgency": "plan tool change"}]
    try:
        execute_propose_work_order({**valid_args, "failure_modes": ["HDF", "TWF"],
                                    "action_code": "REPLACE_TOOL_INSERT", "part_id": "INS-CRB-100"}, dwa)
    except ValueError:
        rejected += 1
    assert execute_propose_work_order({**valid_args, "failure_modes": ["HDF", "TWF"]}, dwa)["status"]
    # zwykły krok serwisowy nie jest blokowany
    assert execute_propose_work_order({**valid_args, "summary": "Disconnect the faulty sensor cable and inspect it."}, reg)

    assert rejected == 15, f"Expected 15 ceiling rejections, got {rejected}"
    print(f"  [Pass 5/5] M7 ceiling boundary rejections OK ({rejected}/15 violations blocked)")

    # 6. Simulate deterministic multi-turn tool calling loop
    sim_msgs = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": "Triage UDI 4362"},
    ]
    # Turn 1: Model calls lookup_machine_history
    sim_tc1 = {
        "role": "assistant",
        "content": None,
        "tool_calls": [{
            "id": "call_1",
            "type": "function",
            "function": {"name": "lookup_machine_history", "arguments": json.dumps({"product_type": "M", "udi": 4362})},
        }],
    }
    sim_msgs.append(sim_tc1)
    res_1 = execute_tool("lookup_machine_history", {"product_type": "M", "udi": 4362}, csv_path)
    sim_msgs.append({"role": "tool", "tool_call_id": "call_1", "content": json.dumps(res_1)})

    # Turn 2: Model calls check_spare_parts_inventory
    sim_tc2 = {
        "role": "assistant",
        "content": None,
        "tool_calls": [{
            "id": "call_2",
            "type": "function",
            "function": {"name": "check_spare_parts_inventory", "arguments": json.dumps({"failure_mode": "HDF"})},
        }],
    }
    sim_msgs.append(sim_tc2)
    res_2 = execute_tool("check_spare_parts_inventory", {"failure_mode": "HDF"}, csv_path)
    sim_msgs.append({"role": "tool", "tool_call_id": "call_2", "content": json.dumps(res_2)})

    # Turn 3: Model calls propose_work_order
    sim_tc3 = {
        "role": "assistant",
        "content": None,
        "tool_calls": [{
            "id": "call_3",
            "type": "function",
            "function": {"name": "propose_work_order", "arguments": json.dumps(valid_args)},
        }],
    }
    sim_msgs.append(sim_tc3)
    res_3 = execute_tool("propose_work_order", valid_args, csv_path, tel["findings"])
    assert res_3["status"] == "GATED_BY_OPERATOR_CAPABILITY_CEILING"
    sim_msgs.append({"role": "tool", "tool_call_id": "call_3", "content": json.dumps(res_3)})

    # Turn 4: Model outputs final answer
    sim_msgs.append({
        "role": "assistant",
        "content": f"Diagnosis complete. Work order proposed with parameter digest {res_3['parametry_digest'][:16]}... Gated pending M7 operator grant.",
    })
    assert len(sim_msgs) == 9
    print("  [Bonus] Multi-turn deterministic tool loop simulation OK (4 turns, 3 tool calls)")

    print("\nALL AGENT TRIAGE SELF-CHECKS PASSED!")
    return True


def main():
    ap = argparse.ArgumentParser(description="SIMON Autonomous Industrial Triage Agent (M7 Tool-Calling)")
    ap.add_argument("--selfcheck", action="store_true", help="Run deterministic offline self-checks")
    ap.add_argument("--gateway", default="http://127.0.0.1:18600", help="SIMON gateway URL")
    ap.add_argument("--model", default="gemma-4-26b-a4b-it-bf16", help="Gemma 4 model identifier")
    ap.add_argument("--csv", default=str(find_default_csv()), help="Path to ai4i2020.csv")
    ap.add_argument("--udi", type=int, default=4362, help="UDI to diagnose")
    ap.add_argument("--max-turns", type=int, default=4, help="Max conversation turns")
    ap.add_argument("--out", help="Optional output JSON path")
    args = ap.parse_args()

    if args.selfcheck or len(sys.argv) == 1:
        selfcheck()
        return

    csv_path = Path(args.csv)
    res = run_triage_agent(args.gateway, args.model, args.udi, csv_path, max_turns=args.max_turns)
    output_str = json.dumps(res, indent=2)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(output_str)
        print(f"Results written to {args.out}")
    else:
        print(output_str)


if __name__ == "__main__":
    main()
