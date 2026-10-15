#!/usr/bin/env python3
"""Local bridge from a SIMON node to AMD Token Factory (AMD-hosted models on AMD GPUs, Radeon Cloud).

Why a bridge:
- Token Factory does not answer from some networks (from Poland it times out); it does through an egress
  proxy (we use a ProtonVPN gluetun HTTP proxy). Set TF_PROXY to that proxy.
- The SIMON node forwards only whitelisted chat fields, so it cannot switch off Gemma 4's thinking. The bridge
  adds chat_template_kwargs {"enable_thinking": false} (thinking would otherwise eat the token budget).
- The team API key stays in a 600 file on this host; the node never sees it.

    TF_KEY_FILE=~/.simon/tokenfactory.key TF_PROXY=http://127.0.0.1:18888 python3 tools/amd/tokenfactory_bridge.py

Then: simon --role node --model-url http://127.0.0.1:18460 --model-hash <Token Factory model id> ...
Listens on 127.0.0.1 only. Logs request lines, never prompts or answers.
"""
import json, os, urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BASE = os.environ.get("TF_BASE", "https://developer.amd.com.cn/radeon/api/v1").rstrip("/")
PORT = int(os.environ.get("TF_BRIDGE_PORT", "18460"))
KEY = open(os.path.expanduser(os.environ.get("TF_KEY_FILE", "~/.simon/tokenfactory.key"))).read().strip()
PROXY = os.environ.get("TF_PROXY", "")
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({"http": PROXY, "https": PROXY} if PROXY else {}))
TIMEOUT = int(os.environ.get("TF_TIMEOUT", "45"))  # per attempt; measured: answers in 2.5-11 s, ~1 in 6 hangs
TRIES = int(os.environ.get("TF_TRIES", "3"))


def prepare(body):
    """The request as sent to Token Factory: thinking off unless the caller set it."""
    req = json.loads(body or b"{}")
    req.setdefault("chat_template_kwargs", {}).setdefault("enable_thinking", False)
    print(f"[tf-bridge] chat fields={sorted(req)} model={req.get('model')!r}", flush=True)  # names only, no text
    return json.dumps(req).encode()


class H(BaseHTTPRequestHandler):
    def _relay(self, method, body=None):
        path = self.path[3:] if self.path.startswith("/v1") else self.path   # accept /v1/... and /...
        req = urllib.request.Request(BASE + path, data=body, method=method,
                                     headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
        for attempt in range(TRIES):  # the VPN path drops or stalls a request now and then: retry it
            try:
                with OPENER.open(req, timeout=TIMEOUT) as r:
                    code, data = r.status, r.read()
                break
            except urllib.error.HTTPError as e:
                code, data = e.code, e.read()
                break
            except OSError as e:  # proxy or upstream down: say so, never invent an answer
                code, data = 502, json.dumps({"error": {"message": f"token factory unreachable after "
                                                                  f"{attempt + 1} tries: {e}"}}).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self._relay("GET")

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        self._relay("POST", prepare(body) if self.path.endswith("/chat/completions") else body)

    def log_message(self, fmt, *a):
        print(f"[tf-bridge] {fmt % a}", flush=True)


def selfcheck():
    assert json.loads(prepare(b'{"model":"m"}'))["chat_template_kwargs"] == {"enable_thinking": False}
    assert json.loads(prepare(b'{"chat_template_kwargs":{"enable_thinking":true}}'))["chat_template_kwargs"]["enable_thinking"]


if __name__ == "__main__":
    selfcheck()
    print(f"[tf-bridge] http://127.0.0.1:{PORT} -> {BASE} (proxy: {PROXY or 'none'})", flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
