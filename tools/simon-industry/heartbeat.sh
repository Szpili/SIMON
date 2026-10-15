#!/usr/bin/env bash
# Demo heartbeat: one ticket per Gemma 4 node every run, a different flagged record each time, so the public
# /network page shows recent signed jobs. Records come from the predeclared sample (15 flagged ones).
# Nodes: RTX 3090 (QAT q4_0), AMD MI300X droplet (BF16), AMD Token Factory (Gemma 4 31B, via the VPN bridge).
set -u
cd "$(dirname "$0")"
k=$(( $(date +%s) / 900 % 15 ))
python3 tickets.py --csv data/ai4i2020.csv \
    --model gemma-4-26b-a4b-it-qat-q4_0 --model gemma-4-26b-a4b-it-bf16 --model gemma-4-31b-it-amd-tokenfactory \
    --pick "$k" --pick "$(( (k + 5) % 15 ))" --pick "$(( (k + 10) % 15 ))" \
    --out "${HEARTBEAT_OUT:-$HOME/.simon/heartbeat.jsonl}"
