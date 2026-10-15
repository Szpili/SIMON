#!/usr/bin/env python3
"""SIMON API — an OpenAI-compatible HTTP front for SIMON nodes (local test gateway).

Every chat completion becomes one SIMON job: `simon --role agent ... --json`.
The answer comes back with the node's signed receipt and the client-side verdict.

    SIMON_BIN=target/release/simon SIMON_API_NODES=nodes.json python3 tools/simon-api/simon_api.py
    curl -s 127.0.0.1:18600/v1/chat/completions -d '{"model":"qwen3.8-27b-bf16","messages":[...]}'

nodes.json: {"<model name>": {"bootstrap": "/ip4/.../tcp/.../p2p/<peer_id>", "model_hash": "..."}}
A model may map to a LIST of such nodes: jobs rotate across them, and an unreachable or busy
node hands the job to the next one. The file is re-read when it changes (add a node, no restart).

What a receipt proves here: the node's key signed this exact output for this job
(proof level 0). It does NOT prove which model ran — see docs/THREAT-MODEL.md.
"""
import hashlib, hmac, itertools, json, os, subprocess, threading, time, uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BIN = os.environ.get("SIMON_BIN", "simon")
HOST, PORT = "127.0.0.1", int(os.environ.get("SIMON_API_PORT", "18600"))
TIMEOUT_S = int(os.environ.get("SIMON_API_TIMEOUT", "600"))
IDENTITY = os.environ.get("SIMON_API_IDENTITY", os.path.expanduser("~/.simon/api-client.id"))
# Optional job log for tools/simon-monitor: one JSON line per completed job. Receipt fields and
# timings only — never the prompt or the output.
JOBLOG = os.environ.get("SIMON_API_JOBLOG", "")
_joblog_lock = threading.Lock()
NODES_PATH = os.environ.get("SIMON_API_NODES", "")
NODES, _nodes_mtime, _rr = {}, None, {}
_nodes_lock = threading.Lock()


def load_nodes():
    """{model: [node, ...]}, re-read when nodes.json changes on disk (a node joins without a restart).
    A model may map to one node or to a LIST of nodes: jobs rotate across them."""
    global NODES, _nodes_mtime
    with _nodes_lock:
        m = os.path.getmtime(NODES_PATH) if NODES_PATH else None
        if m != _nodes_mtime:
            raw = json.load(open(NODES_PATH)) if NODES_PATH else {}
            NODES = {k: (v if isinstance(v, list) else [v]) for k, v in raw.items()}
            _nodes_mtime = m
        return NODES


def nodes_in_turn(model):
    """The model's nodes, starting at the next one in rotation."""
    nodes = load_nodes().get(model) or []
    if not nodes:
        return []
    with _nodes_lock:
        i = next(_rr.setdefault(model, itertools.count())) % len(nodes)
    return nodes[i:] + nodes[:i]


# Fields of an OpenAI request that travel to the node (the node keeps the same whitelist).
CHAT_FIELDS = ("messages", "tools", "tool_choice", "parallel_tool_calls", "temperature", "top_p", "stop")


def log_job(model, r):
    if not JOBLOG:
        return
    # the published feed must never carry text: drop such fields even if a future receipt adds them
    rc = {k: v for k, v in r.get("receipt", {}).items() if k not in ("output", "prompt")}
    line = json.dumps({"ts": time.time(), "model": model, "job_id": r.get("job_id"), "node_id": rc.get("node_id"),
                       "prompt_tokens": rc.get("prompt_tokens"), "completion_tokens": rc.get("completion_tokens"),
                       "job_ms": r.get("_job_ms"), "receipt": rc,
                       # same three claims as the API response: the gateway only logs jobs whose
                       # receipt it verified (signature + output binding); execution is not verified
                       "execution_status": "completed", "receipt_status": "signature_valid_output_bound",
                       "verification_status": "not_performed",
                       "proof_level": 0}) + "\n"  # gateway checks signature + output binding, not M1
    with _joblog_lock, open(JOBLOG, "a") as f:  # one write per line under a lock: no torn lines
        f.write(line)


def run_job(node, req, max_tokens):
    # ponytail: one process per request (~1 s libp2p setup each); a long-lived agent if throughput matters.
    # The request goes in on stdin (chat mode): messages and tools never show up in `ps`.
    cmd = [BIN, "--role", "agent", "--bootstrap", node["bootstrap"], "--model-hash", node["model_hash"],
           "--request-file", "-", "--max-tokens", str(max_tokens), "--identity-file", IDENTITY, "--json"]
    chat = json.dumps({k: req[k] for k in CHAT_FIELDS if k in req})
    t0 = time.time()
    p = subprocess.run(cmd, input=chat, capture_output=True, text=True, timeout=TIMEOUT_S)
    job_ms = int((time.time() - t0) * 1000)  # includes time queued on the node (node --max-jobs)
    # --json should print one JSON object, but the CLI also prints a log line first: take the JSON line.
    js = next((l for l in reversed(p.stdout.splitlines()) if l.startswith("{")), None)
    if p.returncode != 0 or js is None:
        raise RuntimeError((p.stderr or p.stdout)[-800:])
    r = json.loads(js)
    r["_job_ms"] = job_ms
    return r


def run_on_nodes(nodes, req, max_tokens):
    """First node that completes the job. Unreachable, busy or a failed check -> the next node.
    A timeout is NOT retried: that node may still be computing, and the job must not run twice."""
    errors = []
    for node in nodes:
        try:
            return run_job(node, req, max_tokens)
        except subprocess.TimeoutExpired:
            raise
        except Exception as e:
            errors.append(str(e))
    raise RuntimeError(("node_zajety: " if all("node_zajety" in e for e in errors) else "")
                       + f"failed on {len(errors)} node(s): {errors[-1]}")


def completion(req, r, t0):
    """Message, finish_reason, usage and the receipt block of one verified SIMON job.

    The agent verified the receipt over the raw `output` string BEFORE this runs; only then is the
    string parsed. Nothing re-encodes it on the trusted path.
    """
    m = json.loads(r["output"])  # chat mode: {"content", "tool_calls"?, "finish_reason"}
    msg = {"role": "assistant", "content": m.get("content")}
    if m.get("tool_calls"):
        msg["tool_calls"] = m["tool_calls"]
    rc = r.get("receipt", {})
    usage = {"prompt_tokens": rc.get("prompt_tokens"), "completion_tokens": rc.get("completion_tokens"),
             "total_tokens": (rc.get("prompt_tokens") or 0) + (rc.get("completion_tokens") or 0)}
    simon = {
        "job_id": r.get("job_id"), "node_id": rc.get("node_id"), "runtime": rc.get("runtime"),
        # Three separate claims; a valid signature is not a verified computation.
        "execution_status": "completed",
        "receipt_status": "signature_valid_output_bound",
        "verification_status": "not_performed",
        "proof_level": 0,  # the gateway does not check M1 bindings (no client tokens here)
        "proof_note": "the node's key signed this exact output (text and tool calls) for this job; "
                      "this does NOT prove which model or precision ran",
        "receipt": rc,
        # the exact string the receipt's output_digest covers: lets the client (or anyone it shows the
        # answer to) re-check the binding offline. Never written to the public job log.
        "signed_output": r.get("output"),
        "client_observed_ms": int((time.time() - t0) * 1000),
        "receipt_hash": (r.get("obserwacja_klienta") or {}).get("receipt_hash"),
    }
    finish = m.get("finish_reason") or ("tool_calls" if m.get("tool_calls") else "stop")
    return msg, finish, usage, simon


def _try(f, *a):
    try:
        return f(*a)
    except Exception as e:  # handed to the streaming loop as a value
        return e


class Handler(BaseHTTPRequestHandler):
    def _send(self, code, obj, headers=()):
        body = json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        for k, v in headers:
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def _err(self, code, msg):
        self._send(code, {"error": {"message": msg, "type": "simon_error"}})

    def _check_auth(self):
        expected_key = os.environ.get("SIMON_API_KEY")
        if not expected_key:
            return True
        auth_header = self.headers.get("Authorization", "")
        # skróty o stałej długości: porównanie bez wycieku długości tokenu
        if hmac.compare_digest(hashlib.sha256(auth_header.encode()).digest(),
                               hashlib.sha256(f"Bearer {expected_key}".encode()).digest()):
            return True
        self._err(401, "unauthorized: invalid or missing Bearer token")
        return False

    def do_GET(self):
        if self.path.startswith("/v1/") or self.path.rstrip("/") in ("/v1/models", "/models"):
            if not self._check_auth():
                return
        if self.path.rstrip("/") in ("/v1/models", "/models"):
            return self._send(200, {"object": "list", "data": [
                {"id": m, "object": "model", "owned_by": "simon", "model_hash": n[0]["model_hash"], "nodes": len(n)}
                for m, n in load_nodes().items()]})
        if self.path == "/health":
            return self._send(200, {"status": "ok", "models": list(load_nodes())})
        self._err(404, "not found")

    def do_POST(self):
        if self.path.startswith("/v1/") or self.path.rstrip("/") in ("/v1/chat/completions", "/chat/completions"):
            if not self._check_auth():
                return
        if self.path.rstrip("/") not in ("/v1/chat/completions", "/chat/completions"):
            return self._err(404, "not found")
        try:
            req = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        except ValueError:
            return self._err(400, "invalid JSON")
        node = nodes_in_turn(req.get("model"))  # the node list, in rotation order
        if not node:
            return self._err(404, f"unknown model {req.get('model')!r}; see /v1/models")
        msgs = req.get("messages")
        if not isinstance(msgs, list) or not msgs:
            return self._err(400, "empty messages")
        if req.get("stream"):
            return self._stream(node, req)
        t0 = time.time()
        try:
            r = run_on_nodes(node, req, int(req.get("max_tokens") or 512))
        except subprocess.TimeoutExpired:
            return self._err(504, "SIMON job timed out")
        except Exception as e:  # node unreachable, verification failure, etc.
            if "node_zajety" in str(e):  # node queue full: the client should retry, not give up
                return self._send(503, {"error": {"message": f"SIMON node busy: {e}", "type": "simon_busy"}},
                                  headers=[("Retry-After", "5")])
            return self._err(502, f"SIMON job failed: {e}")
        if not r.get("receipt_zweryfikowany"):
            return self._err(502, "receipt did not verify — answer withheld")
        msg, finish, usage, simon = completion(req, r, t0)
        out = {"id": "chatcmpl-" + uuid.uuid4().hex[:24], "object": "chat.completion", "created": int(t0),
               "model": req.get("model"), "choices": [{"index": 0, "finish_reason": finish, "message": msg}],
               "usage": usage, "simon": simon}
        log_job(req.get("model"), r)
        self._send(200, out, headers=[("x-simon-job-id", str(r.get("job_id"))),
                                       ("x-simon-node-id", str(simon["node_id"]))])

    def _stream(self, node, req):
        # ponytail: streaming after the fact. A libp2p request-response job returns one reply, so the
        # whole answer arrives at once; the role chunk goes out immediately and a keepalive comment every
        # few seconds, so a client does not time out while the node computes. Token-by-token streaming
        # needs a streaming protocol between agent and node.
        cid, t0 = "chatcmpl-" + uuid.uuid4().hex[:24], time.time()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()

        def ev(delta=None, finish=None, extra=None, raw=None):
            body = raw if raw is not None else {"id": cid, "object": "chat.completion.chunk", "created": int(t0),
                                                "model": req.get("model"),
                                                "choices": [{"index": 0, "delta": delta or {}, "finish_reason": finish}]}
            if extra:
                body.update(extra)
            self.wfile.write(b"data: " + json.dumps(body, ensure_ascii=False).encode() + b"\n\n")
            self.wfile.flush()

        ev({"role": "assistant", "content": ""})
        box = {}
        th = threading.Thread(target=lambda: box.update(r=_try(run_on_nodes, node, req, int(req.get("max_tokens") or 512))))
        th.start()
        while th.is_alive():
            th.join(5)
            if th.is_alive():
                self.wfile.write(b": simon job running\n\n")
                self.wfile.flush()
        r = box["r"]
        if isinstance(r, Exception) or not r.get("receipt_zweryfikowany"):
            err = str(r) if isinstance(r, Exception) else "receipt did not verify — answer withheld"
            ev(raw={"error": {"message": f"SIMON job failed: {err}", "type": "simon_error"}})
        else:
            msg, finish, usage, simon = completion(req, r, t0)
            if msg.get("content"):
                ev({"content": msg["content"]})
            if msg.get("tool_calls"):
                ev({"tool_calls": [dict(tc, index=i) for i, tc in enumerate(msg["tool_calls"])]})
            ev(finish=finish, extra={"usage": usage, "simon": simon})
            log_job(req.get("model"), r)
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()

    def log_message(self, fmt, *args):  # no prompt content in logs, just the request line
        print(f"[simon-api] {self.address_string()} {fmt % args}", flush=True)


if __name__ == "__main__":
    if not load_nodes():
        raise SystemExit("set SIMON_API_NODES to a nodes.json file")
    print(f"[simon-api] http://{HOST}:{PORT}  models: " + ", ".join(f"{m} ({len(n)})" for m, n in NODES.items()), flush=True)
    ThreadingHTTPServer((HOST, PORT), Handler).serve_forever()
