#!/usr/bin/env bash
# Demo heartbeat: one ticket per Gemma 4 node every run, a different flagged record each time, so the public
# /network page shows recent signed jobs. Records come from the predeclared sample (15 flagged ones).
set -u
cd "$(dirname "$0")"
k=$(( $(date +%s) / 900 % 15 ))
python3 tickets.py --csv data/ai4i2020.csv --model gemma-4-26b-a4b-it-qat-q4_0 --model gemma-4-26b-a4b-it-bf16 \
    --pick "$k" --pick "$(( (k + 7) % 15 ))" --out "${HEARTBEAT_OUT:-$HOME/.simon/heartbeat.jsonl}"
