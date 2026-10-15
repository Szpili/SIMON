#!/usr/bin/env python3
"""SIMON network monitor — read-only JSON for the /network page of the web UI.

One hub on the demo host. It samples local GPUs with nvidia-smi and tails the job log
written by tools/simon-api (SIMON_API_JOBLOG). Node state comes only from real jobs:
"active" (a job finished in the last ACTIVE_S seconds) or "last job N s ago" — never a
guessed "online".

    SIMON_MONITOR_CONFIG=monitor.json SIMON_API_JOBLOG=jobs.jsonl python3 tools/simon-monitor/simon_monitor.py

monitor.json:
    {"nodes": [{"label": "RTX 3090 · Poland", "model": "qwen2.5-3b-q8", "gpu": 1,
                "shared_with": "Strata"}]}
  model       — the gateway's model name; jobs are matched to the node by it
  node_id     — optional: match jobs by the receipt's node_id instead (two nodes serving one model)
  gpu         — nvidia-smi index (PCI order) to sample, or null for a remote node
  stats_url   — remote node: URL returning {name, util, vram_used_mb, vram_total_mb, temp_c, power_w,
                power_limit_w} (tools/amd/demo_node.sh serves rocm-smi this way through its SSH tunnel)
  shared_with — shown on the card: GPU load is not all SIMON's
  canary      — optional: status file written by tools/simon-canary (operator self-check of the weights)

GET /api/monitor → {generated_at, nodes[], jobs[] (last 50), totals}. No prompts, no
outputs, no addresses: only what is already in a receipt plus GPU telemetry.
"""
import json, os, subprocess, threading, time, urllib.request
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HOST, PORT = "127.0.0.1", int(os.environ.get("SIMON_MONITOR_PORT", "8081"))
CONFIG = json.load(open(os.environ["SIMON_MONITOR_CONFIG"])) if os.environ.get("SIMON_MONITOR_CONFIG") else {"nodes": []}
JOBLOG = os.environ.get("SIMON_API_JOBLOG", "")
SAMPLE_S, HISTORY, ACTIVE_S, FEED = 2.0, 90, 10, 50
CANARY_STALE_S = int(os.environ.get("SIMON_CANARY_STALE_S", "1800"))  # 3x the 10-min timer


def read_canary(path):
    """Last integrity-canary result for a node, or None when not configured."""
    if not path:
        return None
    try:
        c = json.load(open(os.path.expanduser(path)))
    except (OSError, ValueError):
        return {"verdict": "missing"}
    age = round(time.time() - c.get("ts", 0))
    keep = {k: c.get(k) for k in ("verdict", "hits", "of", "served_model", "confirmed")}
    keep["age_s"] = age
    if age > CANARY_STALE_S:
        keep["verdict"] = "stale"  # a stopped canary must not keep showing its last "ok"
    return keep

GPU_FIELDS = "index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit"


def num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None  # "[N/A]", "[Not Supported]", empty


def parse_nvidia_csv(text):
    """nvidia-smi --query-gpu=GPU_FIELDS --format=csv,noheader,nounits → {index: {...}}."""
    out = {}
    for line in text.strip().splitlines():
        f = [x.strip() for x in line.split(",")]
        if len(f) != 8 or num(f[0]) is None:
            continue
        out[int(f[0])] = {"name": f[1], "util": num(f[2]), "vram_used_mb": num(f[3]), "vram_total_mb": num(f[4]),
                          "temp_c": num(f[5]), "power_w": num(f[6]), "power_limit_w": num(f[7])}
    return out


def sample_gpus():
    try:
        r = subprocess.run(["nvidia-smi", f"--query-gpu={GPU_FIELDS}", "--format=csv,noheader,nounits"],
                           capture_output=True, text=True, timeout=5)
        return parse_nvidia_csv(r.stdout) if r.returncode == 0 else {}
    except (OSError, subprocess.TimeoutExpired):
        return {}  # no driver / no nvidia-smi → cards show "no telemetry"


def fetch_stats(url):
    """Telemetry of a remote node, or None (unreachable / error): the card then says so, no numbers invented."""
    try:
        with urllib.request.urlopen(url, timeout=1.5) as r:
            d = json.loads(r.read())
        return d if "error" not in d else None
    except (OSError, ValueError):
        return None


class JobTail:
    """Reads only complete new lines; restarts from 0 if the file was truncated or rotated."""

    def __init__(self, path):
        self.path, self.offset, self.buf = path, 0, b""

    def read_new(self):
        if not self.path or not os.path.exists(self.path):
            return []
        size = os.path.getsize(self.path)
        if size < self.offset:
            self.offset, self.buf = 0, b""
        with open(self.path, "rb") as f:
            f.seek(self.offset)
            data = f.read()
        self.offset += len(data)
        *lines, self.buf = (self.buf + data).split(b"\n")
        out = []
        for l in lines:
            try:
                out.append(json.loads(l))
            except ValueError:
                pass  # a torn line from a crash: skip it, don't stop the feed
        return out


class State:
    def __init__(self, nodes):
        self.lock = threading.Lock()
        self.nodes = nodes
        self.hist = {i: {k: deque(maxlen=HISTORY) for k in ("util", "vram", "temp", "power", "tok_s")} for i in range(len(nodes))}
        self.gpu = {}
        self.jobs = deque(maxlen=FEED)
        self.totals = {"jobs": 0, "completion_tokens": 0, "per_node": {}}
        self.last_job = {}

    def add_job(self, j):
        idx = next((i for i, n in enumerate(self.nodes) if n.get("node_id") and n["node_id"] == j.get("node_id")), None)
        if idx is None:
            idx = next((i for i, n in enumerate(self.nodes) if not n.get("node_id") and n.get("model") == j.get("model")), None)
        ms = j.get("job_ms") or 0
        ct = j.get("completion_tokens") or 0
        j["node_label"] = self.nodes[idx]["label"] if idx is not None else None
        j["tok_s"] = round(ct / (ms / 1000), 1) if ms else None  # gateway-timed job: includes libp2p setup and time queued on the node
        self.jobs.appendleft(j)
        self.totals["jobs"] += 1
        self.totals["completion_tokens"] += ct
        key = j["node_label"] or "unassigned"
        self.totals["per_node"][key] = self.totals["per_node"].get(key, 0) + 1
        if idx is not None:
            self.last_job[idx] = j.get("ts") or time.time()
            if j["tok_s"] is not None:
                self.hist[idx]["tok_s"].append(j["tok_s"])

    def snapshot(self):
        now = time.time()
        with self.lock:
            nodes = []
            for i, n in enumerate(self.nodes):
                g = self.gpu.get(n.get("gpu")) if n.get("gpu") is not None else self.gpu.get(n.get("stats_url"))
                last = self.last_job.get(i)
                nodes.append({
                    "label": n["label"], "model": n.get("model"), "shared_with": n.get("shared_with"),
                    "telemetry": ("ok" if g else "unavailable") if (n.get("gpu") is not None or n.get("stats_url")) else "remote",
                    "state": "active" if last and now - last <= ACTIVE_S else ("idle" if last else "no_jobs"),
                    "last_job_age_s": round(now - last) if last else None,
                    "gpu": g, "history": {k: list(v) for k, v in self.hist[i].items()},
                    "canary": read_canary(n.get("canary")),
                })
            return {"generated_at": now, "nodes": nodes, "jobs": list(self.jobs),
                    "totals": json.loads(json.dumps(self.totals))}


def loop(st, tail):
    while True:
        gpus = sample_gpus() if any(n.get("gpu") is not None for n in st.nodes) else {}
        for n in st.nodes:
            if n.get("stats_url"):
                gpus[n["stats_url"]] = fetch_stats(n["stats_url"])
        new = tail.read_new()
        with st.lock:
            st.gpu = gpus
            for i, n in enumerate(st.nodes):
                key = n.get("gpu") if n.get("gpu") is not None else n.get("stats_url")
                g = gpus.get(key) if key is not None else None
                h = st.hist[i]
                if key is not None and not g:
                    for k in ("util", "temp", "power", "vram"):
                        h[k].append(None)  # a missed sample is a visible gap, not a squeezed timeline
                if g:
                    h["util"].append(g["util"]); h["temp"].append(g["temp_c"]); h["power"].append(g["power_w"])
                    h["vram"].append(round(100 * g["vram_used_mb"] / g["vram_total_mb"], 1) if g["vram_total_mb"] else None)
            for j in new:
                st.add_job(j)
        time.sleep(SAMPLE_S)


def make_handler(st):
    class H(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path.split("?")[0].rstrip("/") not in ("/api/monitor", "/simon-monitor/api/monitor"):
                self.send_error(404)
                return
            body = json.dumps(st.snapshot()).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Access-Control-Allow-Origin", "*")  # read-only public data
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *a):
            pass
    return H


def selfcheck():
    assert fetch_stats("http://127.0.0.1:1/stats") is None  # unreachable remote -> no numbers
    import tempfile
    cp = tempfile.mktemp()
    json.dump({"verdict": "ok", "hits": 8, "of": 8, "ts": time.time()}, open(cp, "w"))
    assert read_canary(cp)["verdict"] == "ok" and read_canary(None) is None
    json.dump({"verdict": "ok", "hits": 8, "of": 8, "ts": time.time() - CANARY_STALE_S - 5}, open(cp, "w"))
    assert read_canary(cp)["verdict"] == "stale" and read_canary(cp + ".nope")["verdict"] == "missing"
    os.remove(cp)
    g = parse_nvidia_csv("1, NVIDIA GeForce RTX 3090, 37, 22976, 24576, 41, 112.40, 350.00\n"
                         "0, NVIDIA GeForce GTX TITAN X, [N/A], 7788, 12288, 30, [N/A], 250.00\nbad line\n")
    assert g[1]["util"] == 37 and g[1]["vram_total_mb"] == 24576 and g[0]["util"] is None and len(g) == 2
    import tempfile
    p = tempfile.mktemp()
    t = JobTail(p)
    open(p, "wb").write(b'{"a":1}\n{"a":')
    assert [x["a"] for x in t.read_new()] == [1]          # partial last line held back
    open(p, "ab").write(b'2}\n')
    assert [x["a"] for x in t.read_new()] == [2]          # completed on the next read
    open(p, "wb").write(b'{"a":3}\n')
    assert [x["a"] for x in t.read_new()] == [3]          # truncation → restart
    os.remove(p)
    s = State([{"label": "n", "model": "m", "gpu": None}])
    s.add_job({"model": "m", "job_ms": 2000, "completion_tokens": 100, "ts": time.time()})
    snap = s.snapshot()
    assert snap["nodes"][0]["state"] == "active" and snap["jobs"][0]["tok_s"] == 50.0 and snap["totals"]["jobs"] == 1
    s2 = State([{"label": "a", "model": "m", "node_id": "A", "gpu": None}, {"label": "b", "model": "m", "node_id": "B", "gpu": None}])
    s2.add_job({"model": "m", "node_id": "B", "job_ms": 1000, "completion_tokens": 10, "ts": time.time()})
    assert [n["state"] for n in s2.snapshot()["nodes"]] == ["no_jobs", "active"]  # same model, matched by node_id


if __name__ == "__main__":
    selfcheck()
    st = State(CONFIG.get("nodes", []))
    threading.Thread(target=loop, args=(st, JobTail(JOBLOG)), daemon=True).start()
    print(f"[simon-monitor] http://{HOST}:{PORT}/api/monitor  nodes: {len(st.nodes)}  joblog: {JOBLOG or '-'}", flush=True)
    ThreadingHTTPServer((HOST, PORT), make_handler(st)).serve_forever()
