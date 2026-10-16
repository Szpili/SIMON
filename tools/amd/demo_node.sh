#!/usr/bin/env bash
# SIMON demo node on an AMD Instinct MI300X droplet — one command from a fresh droplet to a node
# serving the public demo. Run on the demo host (where simon-demo-api and simon-monitor run).
#
#   tools/amd/demo_node.sh up   <droplet-ip> <name> [label]   # e.g. up 165.245.1.2 karol
#   tools/amd/demo_node.sh down <name>                         # detach from the demo (droplet: destroy in console)
#
# Droplet: AMD Developer Cloud, 1× MI300X, image "Quick Start vLLM" (vLLM in docker container `rocm`),
# created with our SSH key (~/.ssh/amd_devcloud.pub). Nothing is exposed to the internet:
#   - vLLM :8000 is blocked on eth0 (IPv4 DOCKER-USER, IPv6 INPUT), re-applied on every start;
#   - the SIMON node listens on 127.0.0.1 only; the demo gateway reaches it through an SSH tunnel.
# The node key lives on the demo host (~/.simon/amd-<name>.key) and is uploaded each time, so the
# node keeps the same peer id / node_id across droplets: nodes.json and the monitor stay valid.
set -euo pipefail

# Default: Gemma 4 26B-A4B-it in BF16 (ungated on Hugging Face). served name = model hash.
MODEL=${MODEL:-gemma-4-26b-a4b-it-bf16}
HF_MODEL=${HF_MODEL:-google/gemma-4-26B-A4B-it}
# Gemma 4 thinks by default: off by default on the server (the node forwards only whitelisted fields),
# and the gemma4 parser keeps any reasoning out of the signed content.
VLLM_EXTRA=${VLLM_EXTRA:-"--reasoning-parser gemma4 --default-chat-template-kwargs '{\"enable_thinking\": false}'"}
SSH_KEY=${SSH_KEY:-$HOME/.ssh/amd_devcloud}
BIN=${SIMON_BIN:-$HOME/.local/lib/simon/simon}
NODES=${SIMON_API_NODES:-$HOME/.simon/demo-nodes.json}
MONITOR=${SIMON_MONITOR_CONFIG:-$HOME/.simon/monitor.json}
UNITS=$HOME/.config/systemd/user

cmd=${1:-}; shift || true

json_edit() {  # json_edit <name> [entry-json] : drop <name> from nodes.json + monitor.json, then add entry
  python3 - "$NODES" "$MONITOR" "$MODEL" "$@" <<'PY'
import json, sys
nodes_p, mon_p, model, name = sys.argv[1:5]
entry = json.loads(sys.argv[5]) if len(sys.argv) > 5 else None
nodes = json.load(open(nodes_p))
lst = nodes.get(model, [])
lst = [n for n in (lst if isinstance(lst, list) else [lst]) if n.get("name") != name]
mon = json.load(open(mon_p))
mon["nodes"] = [n for n in mon["nodes"] if n.get("name") != name]
if entry:
    lst.append(entry["node"])
    mon["nodes"].append(entry["monitor"])
if lst: nodes[model] = lst
else: nodes.pop(model, None)
for p, d in ((nodes_p, nodes), (mon_p, mon)):
    json.dump(d, open(p + ".tmp", "w"), indent=1)
    __import__("os").replace(p + ".tmp", p)   # atomic: the gateway re-reads nodes.json on change
PY
}

case "$cmd" in
up)
  IP=${1:?usage: up <droplet-ip> <name> [label]}; NAME=${2:?name}; LABEL=${3:-"AMD Instinct MI300X · $NAME"}
  [[ $NAME =~ ^[a-z0-9-]+$ ]] || { echo "name: lowercase letters, digits, dashes" >&2; exit 2; }
  LPORT=$((19100 + $(printf %s "$NAME" | cksum | cut -d' ' -f1) % 800))   # stable local tunnel port per name
  NODEKEY=$HOME/.simon/amd-$NAME.key
  [ -f "$NODEKEY" ] || (umask 077; openssl rand -hex 32 > "$NODEKEY")
  SSH=(ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=accept-new -o ConnectTimeout=15 "root@$IP")
  t0=$(date +%s)

  echo "== 1/5 upload binary + node key (one scp: the image rate-limits ssh with ufw) =="
  scp -q -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=accept-new "$BIN" "$NODEKEY" "root@$IP:/root/"

  echo "== 2/5 lock down :8000, install vLLM + node as systemd services =="
  "${SSH[@]}" MODEL="$MODEL" HF_MODEL="$HF_MODEL" VLLM_EXTRA="$(printf %q "$VLLM_EXTRA")" KEYNAME="$(basename "$NODEKEY")" bash -s <<'REMOTE'
set -euo pipefail
install -d -m 700 /root/.simon && install -m 600 "/root/$KEYNAME" /root/.simon/node.key && rm -f "/root/$KEYNAME"
install -m 755 /root/simon /usr/local/bin/simon && rm -f /root/simon
ldd /usr/local/bin/simon | grep -q 'not found' && { echo "simon: missing libraries on this image" >&2; exit 1; }
docker ps --format '{{.Names}}' | grep -qx rocm || { echo "no running 'rocm' container: wrong image?" >&2; exit 1; }
# Gemma 4 on the Quick Start image: vLLM 0.27.1 needs transformers 5.5.x; the image ships 5.15, where
# head_dim became a per-layer attribute and vLLM's Gemma 4 config conversion fails at start.
case "$HF_MODEL" in *gemma-4*) docker exec rocm pip install -q --root-user-action=ignore "transformers==5.5.3" ;; esac
cat > /usr/local/sbin/simon-lockdown <<'EOF'
#!/bin/sh
# vLLM :8000 is published by docker on 0.0.0.0 and [::]: block it from outside, keep loopback/tunnel.
iptables  -C DOCKER-USER -i eth0 -p tcp --dport 8000 -j DROP 2>/dev/null || iptables  -I DOCKER-USER -i eth0 -p tcp --dport 8000 -j DROP
ip6tables -C INPUT       -i eth0 -p tcp --dport 8000 -j DROP 2>/dev/null || ip6tables -I INPUT       -i eth0 -p tcp --dport 8000 -j DROP
ip6tables -C DOCKER-USER -i eth0 -p tcp --dport 8000 -j DROP 2>/dev/null || ip6tables -I DOCKER-USER -i eth0 -p tcp --dport 8000 -j DROP 2>/dev/null || true
EOF
chmod 755 /usr/local/sbin/simon-lockdown && /usr/local/sbin/simon-lockdown
cat > /etc/systemd/system/simon-vllm.service <<EOF
[Unit]
Description=vLLM ($HF_MODEL) in the Quick Start rocm container, for the SIMON node
After=docker.service
Requires=docker.service
[Service]
ExecStartPre=/usr/local/sbin/simon-lockdown
ExecStart=/usr/bin/docker exec rocm vllm serve $HF_MODEL --served-model-name $MODEL --host 0.0.0.0 --port 8000 \
  --max-model-len 8192 --max-num-seqs 64 --gpu-memory-utilization 0.9 --language-model-only $VLLM_EXTRA
ExecStop=-/usr/bin/docker exec rocm pkill -f "vllm serve"
Restart=always
RestartSec=10
[Install]
WantedBy=multi-user.target
EOF
cat > /etc/systemd/system/simon-node.service <<EOF
[Unit]
Description=SIMON node on the MI300X (loopback only; reached through the demo host's SSH tunnel)
After=simon-vllm.service
[Service]
ExecStartPre=/bin/sh -c 'until curl -sf http://127.0.0.1:8000/v1/models >/dev/null; do sleep 5; done'
TimeoutStartSec=3600
ExecStart=/usr/local/bin/simon --role node --listen /ip4/127.0.0.1/tcp/9001 --model-url http://127.0.0.1:8000 \
  --model-hash $MODEL --key-file /root/.simon/node.key --max-jobs 8
# the Quick Start image keeps no journal on disk: the node logs to a file the demo host can read
StandardOutput=append:/var/log/simon-node.log
StandardError=append:/var/log/simon-node.log
Restart=always
RestartSec=10
[Install]
WantedBy=multi-user.target
EOF
cat > /usr/local/bin/simon-gpustats <<'EOF'
#!/usr/bin/env python3
"""GPU telemetry of this MI300X for the demo's /network page: GET /stats on 127.0.0.1:9101 (via the SSH tunnel)."""
import json, subprocess, time
from http.server import BaseHTTPRequestHandler, HTTPServer
last = [0.0, b"{}"]
def read():
    if time.time() - last[0] > 1:
        c = next(iter(json.loads(subprocess.run(["rocm-smi", "--showuse", "--showmemuse", "--showtemp", "--showpower",
             "--showmeminfo", "vram", "--showproductname", "--showmaxpower", "--json"], capture_output=True, text=True,
             timeout=10).stdout).values()))
        f = lambda k: float(c[k]) if c.get(k) not in (None, "N/A") else None
        last[:] = [time.time(), json.dumps({"name": c.get("Card Series"), "util": f("GPU use (%)"),
            "vram_used_mb": f("VRAM Total Used Memory (B)") / 2**20, "vram_total_mb": f("VRAM Total Memory (B)") / 2**20,
            "temp_c": f("Temperature (Sensor junction) (C)"), "power_w": f("Current Socket Graphics Package Power (W)"),
            "power_limit_w": f("Max Graphics Package Power (W)")}).encode()]
    return last[1]
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        try:
            body = read(); self.send_response(200)
        except Exception as e:  # rocm-smi missing or busy: say so, never invent numbers
            body = json.dumps({"error": str(e)[:200]}).encode(); self.send_response(503)
        self.send_header("Content-Type", "application/json"); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
HTTPServer(("127.0.0.1", 9101), H).serve_forever()
EOF
chmod 755 /usr/local/bin/simon-gpustats
cat > /etc/systemd/system/simon-gpustats.service <<EOF
[Unit]
Description=MI300X telemetry for the SIMON demo monitor (127.0.0.1:9101, reached through the SSH tunnel)
[Service]
ExecStart=/usr/local/bin/simon-gpustats
Restart=always
RestartSec=5
[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now simon-gpustats.service
systemctl enable --now simon-vllm.service
systemctl enable simon-node.service && systemctl restart --no-block simon-node.service
REMOTE

  echo "== 3/5 wait for vLLM (first start downloads the weights: ~52 GB for Gemma 4 26B BF16) =="
  until "${SSH[@]}" 'curl -sf http://127.0.0.1:8000/v1/models >/dev/null && systemctl is-active -q simon-node'; do
    sleep 20; printf '  %ss  %s\n' $(( $(date +%s) - t0 )) \
      "$("${SSH[@]}" 'journalctl -u simon-vllm -n 1 --no-pager -o cat 2>/dev/null | cut -c1-110' || echo '(ssh busy)')"
  done
  sleep 5
  PEER=$("${SSH[@]}" "grep -ohE 'peer_id=12D3Koo[A-Za-z0-9]+' /var/log/simon-node.log | tail -1 | cut -d= -f2")
  NODE_ID=$("${SSH[@]}" "grep -ohE 'node_id=[0-9a-f]+' /var/log/simon-node.log | tail -1 | cut -d= -f2")
  [ -n "$PEER" ] && [ -n "$NODE_ID" ] || { echo "no peer_id/node_id in the node log" >&2; exit 1; }

  echo "== 4/5 SSH tunnel 127.0.0.1:$LPORT -> droplet 127.0.0.1:9001 (user unit, restarts on drop) =="
  mkdir -p "$UNITS"
  cat > "$UNITS/simon-amd-tunnel-$NAME.service" <<EOF
[Unit]
Description=SSH tunnel to the SIMON MI300X node '$NAME' ($IP)
[Service]
ExecStart=/usr/bin/ssh -i $SSH_KEY -N -o BatchMode=yes -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 \
  -o ServerAliveCountMax=3 -o StrictHostKeyChecking=accept-new -L 127.0.0.1:$LPORT:127.0.0.1:9001 -L 127.0.0.1:$((LPORT + 1000)):127.0.0.1:9101 \
  -L 127.0.0.1:$((LPORT + 2000)):127.0.0.1:8000 root@$IP
Restart=always
# the image rate-limits ssh (ufw limit): fast retries keep re-arming the block
RestartSec=30
[Install]
WantedBy=default.target
EOF
  systemctl --user daemon-reload && systemctl --user enable -q "simon-amd-tunnel-$NAME" && systemctl --user restart "simon-amd-tunnel-$NAME"

  echo "== 5/5 register in the demo: nodes.json (gateway re-reads it) + monitor =="
  json_edit "$NAME" "$(python3 -c 'import json,sys; n,p,m,lp,nid,lab=sys.argv[1:]; print(json.dumps({
    "node": {"name": n, "bootstrap": f"/ip4/127.0.0.1/tcp/{lp}/p2p/{p}", "model_hash": m, "node_id": nid},
    "monitor": {"name": n, "label": lab, "model": m, "gpu": None, "node_id": nid,
                "stats_url": f"http://127.0.0.1:{int(lp) + 1000}/stats"}}))' \
    "$NAME" "$PEER" "$MODEL" "$LPORT" "$NODE_ID" "$LABEL")"
  systemctl --user restart simon-monitor 2>/dev/null || true
  sleep 3
  for _ in $(seq 1 18); do ss -ltn | grep -q ":$LPORT " && break; sleep 10; done   # tunnel up (ufw may delay it)
  echo "-- smoke test through the public gateway --"
  curl -s -m 300 127.0.0.1:${SIMON_API_PORT:-18600}/v1/chat/completions -H 'Content-Type: application/json' \
    -d "{\"model\":\"$MODEL\",\"max_tokens\":64,\"messages\":[{\"role\":\"user\",\"content\":\"In one sentence: what does a signed inference receipt prove?\"}]}" \
    | python3 -c 'import json,sys; d=json.load(sys.stdin); s=d.get("simon") or {}
print(d.get("error") or "OK node_id=%s receipt=%s %s ms\n%s" % (str(s.get("node_id"))[:16], s.get("receipt_status"),
      s.get("client_observed_ms"), d["choices"][0]["message"]["content"]))'
  echo "== up in $(( $(date +%s) - t0 ))s: $LABEL  peer=$PEER =="
  ;;
down)
  NAME=${1:?usage: down <name>}
  json_edit "$NAME"
  systemctl --user disable --now "simon-amd-tunnel-$NAME" 2>/dev/null || true
  rm -f "$UNITS/simon-amd-tunnel-$NAME.service"; systemctl --user daemon-reload
  systemctl --user restart simon-monitor 2>/dev/null || true
  echo "detached '$NAME' (key kept in ~/.simon/amd-$NAME.key). Destroy the droplet in the AMD console: it bills by the hour."
  ;;
*) sed -n 2,13p "$0"; exit 2 ;;
esac
