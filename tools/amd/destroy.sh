#!/usr/bin/env bash
# End the paid session: local cleanup first, then destroy the VM (provider-specific).
set -euo pipefail

echo "== local cleanup =="
for port in 18301 9001; do
  p=$(ss -tlnp 2>/dev/null | awk -v P=":$port " '$0 ~ P' | grep -oE 'pid=[0-9]+' | head -1 | cut -d= -f2)
  [ -n "${p:-}" ] && kill "$p" 2>/dev/null && echo "stopped :$port"
done

cat <<'EOF'
== destroy the VM now (stops the hourly meter) ==
Provider-specific — use the AMD Developer Cloud console or CLI, e.g.:
  - stop the instance
  - delete it if the session is over
Do NOT leave it running between sessions; credits are spent by the hour.
EOF
