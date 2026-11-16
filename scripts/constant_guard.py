#!/usr/bin/env python3
"""Scans for hardcoded token divisors in throughput calculations.

Detects division of token counts by numeric literals, excluding:
  - comments (incident descriptions may remain),
  - division by 1000 or 1000.0 (ms to seconds conversion),
  - division by variables (e.g. measured `gen_ms`).

Returns 1 if any violation is found.
"""
import pathlib
import re
import sys

# `tokens...` divided by a numeric literal
PATTERN = re.compile(r"tokens?[_a-z]*\s*(?:as\s+f64\s*)?/\s*([0-9]+(?:\.[0-9]+)?)")
ALLOWED_DIVISORS = {"1000", "1000.0"}

root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "crates")
violations = []

for path in root.rglob("*.rs"):
    if "target" in path.parts:
        continue
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if line.lstrip().startswith("//"):
            continue
        for divisor in PATTERN.findall(line):
            if divisor in ALLOWED_DIVISORS:
                continue
            violations.append(f"{path}:{line_no}: token division by literal constant {divisor} -> {line.strip()}")

if violations:
    print("ERROR: token count divided by literal constant — signature of missing measurement:")
    for v in violations:
        print("  " + v)
    sys.exit(1)

print(f"constant guard: clean ({root})")
