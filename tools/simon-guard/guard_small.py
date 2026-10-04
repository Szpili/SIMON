#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""SIMON mały guard — dla słabego sprzętu (CPU), bez GPU.

Warstwy:
  1. REGUŁY (deterministyczne, `pii_pl.Anonimizer`): PII strukturalne (PESEL/NIP/REGON/IBAN/
     TEL/EMAIL/…) + SEKRETY (KLUCZ_API/TOKEN/HASLO/CONNSTR/…). 0 ms, sumy kontrolne.
  2. MODEL (Bielik-Guard 0.1B/0.5B, apache-2.0): toksyczność w 5 kategoriach
     (hate/vulgar/sex/crime/self-harm). ~0,1–0,3 s na CPU, ~0,5–1,7 GB.

CZEGO NIE ROBI (uczciwie): NIE łapie prompt injection (to warstwa semantyczna LLM 11B/agent —
patrz brain `szyfrowanie-promptow-pgp-vs-injection`, `anon-guard-simon-bielik`). Guard widzi
treść ⇒ RULES #1: kto hostuje guard, ten widzi prompt.

Użycie:
  python3 guard_small.py "tekst do sprawdzenia"
  echo "tekst" | python3 guard_small.py --stdin
  python3 guard_small.py --serve --port 19301          # POST /guard {"text": "..."}
  python3 guard_small.py --bench ~/.openclaw/workspace/anon-guard/test-bielik-guard.py
"""
from __future__ import annotations
import argparse, json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = os.path.dirname(os.path.abspath(__file__))
ANON = os.path.expanduser("~/.openclaw/workspace/anon-guard")
for _p in (HERE, ANON):  # najpierw lokalny pii_pl.py, potem workspace
    if _p and _p not in sys.path:
        sys.path.insert(0, _p)
from pii_pl import Anonimizer  # noqa: E402

SEKRETY = {"KLUCZ_SSH", "KLUCZ_API", "TOKEN", "CONNSTR", "SEKRET", "HASLO"}
PII = {"IBAN", "PESEL", "PESEL?", "NIP", "REGON", "TEL", "EMAIL", "SYGN", "KW", "KOD"}
TOKS_KAT = ("hate", "vulgar", "sex", "crime", "self-harm")
DEFAULT_DIR = os.path.expanduser("~/.cache/bielik-guard/0.5B")


class Guard:
    def __init__(self, guard_dir: str = DEFAULT_DIR, prog: float = 0.5, threads: int = 8,
                 use_model: bool = True) -> None:
        os.environ.setdefault("OMP_NUM_THREADS", str(threads))
        self.prog = prog
        self.clf = None
        if use_model:
            from transformers import pipeline  # import dopiero gdy potrzebny
            self.clf = pipeline("text-classification", model=guard_dir, tokenizer=guard_dir,
                                top_k=None, device=-1)

    def klasyfikuj(self, tekst: str) -> dict:
        t0 = time.time()
        _, st = Anonimizer().redact(tekst)
        typy = set(st.get("encje", {}).keys())
        pii = sorted(typy & PII)
        sekret = sorted(typy & SEKRETY)
        t1 = time.time()

        toks: dict[str, float] = {}
        if self.clf is not None:
            for x in self.clf(tekst, truncation=True, max_length=256)[0]:
                toks[x["label"]] = round(float(x["score"]), 4)
        t2 = time.time()

        toks_kat = [k for k, v in toks.items() if v >= self.prog]
        kategorie = pii + sekret + toks_kat
        return {
            "ryzyko": bool(pii or sekret or toks_kat),
            "pii": pii,
            "sekret": sekret,
            "toksycznosc": toks,
            "kategorie": kategorie,
            "ms_reguly": round((t1 - t0) * 1000, 2),
            "ms_model": round((t2 - t1) * 1000, 2),
        }


def _handler(guard: Guard):
    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):  # cisza
            pass

        def do_POST(self):
            if self.path != "/guard":
                self.send_error(404); return
            n = int(self.headers.get("Content-Length", 0))
            try:
                body = json.loads(self.rfile.read(n) or b"{}")
                tekst = body.get("text", "")
                out = guard.klasyfikuj(tekst)
            except Exception as e:  # noqa: BLE001
                out = {"error": str(e)}
            data = json.dumps(out, ensure_ascii=False).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            if self.path == "/health":
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b'{"ok":true}')
            else:
                self.send_error(404)
    return H


def _bench(guard: Guard, sciezka: str) -> None:
    import importlib.util
    spec = importlib.util.spec_from_file_location("tbg", sciezka)
    m = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(m)
    except SystemExit:
        pass
    cases = getattr(m, "CASES", [])
    # metryki reguł: PII/SEKRET; INJECTION reguły nie łapią (raportujemy wprost)
    tp = {k: 0 for k in ("PII", "SEKRET", "INJECTION", "CZYSTE")}
    fp = 0
    lat_r = lat_m = 0.0
    for cid, klasa, tagi, tekst in cases:
        r = guard.klasyfikuj(tekst)
        lat_r += r["ms_reguly"]; lat_m += r["ms_model"]
        if klasa == "PII" and r["pii"]: tp["PII"] += 1
        elif klasa == "SEKRET" and r["sekret"]: tp["SEKRET"] += 1
        elif klasa == "INJECTION" and r["ryzyko"]: tp["INJECTION"] += 1
        elif klasa == "CZYSTE" and r["ryzyko"]: fp += 1
    from collections import Counter
    n = Counter(c[1] for c in cases)
    print(f"przypadków={len(cases)}  {dict(n)}")
    print(f"REGUŁY: PII {tp['PII']}/{n['PII']}  SEKRET {tp['SEKRET']}/{n['SEKRET']}  "
          f"INJECTION {tp['INJECTION']}/{n['INJECTION']}  CZYSTE FP {fp}/{n['CZYSTE']}")
    print(f"latencja średnia: reguły {lat_r/len(cases):.2f} ms, model {lat_m/len(cases):.1f} ms")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("tekst", nargs="?")
    ap.add_argument("--stdin", action="store_true")
    ap.add_argument("--dir", default=DEFAULT_DIR)
    ap.add_argument("--prog", type=float, default=0.5)
    ap.add_argument("--no-model", action="store_true")
    ap.add_argument("--serve", action="store_true")
    ap.add_argument("--port", type=int, default=19301)
    ap.add_argument("--bench")
    a = ap.parse_args()

    if a.serve:
        g = Guard(a.dir, a.prog, use_model=not a.no_model)
        srv = ThreadingHTTPServer(("127.0.0.1", a.port), _handler(g))
        print(f"guard na http://127.0.0.1:{a.port}/guard (loopback; RULES #1: widzi treść)", flush=True)
        srv.serve_forever()
        return 0

    g = Guard(a.dir, a.prog, use_model=not a.no_model)
    if a.bench:
        _bench(g, a.bench); return 0
    tekst = sys.stdin.read() if a.stdin else (a.tekst or "")
    print(json.dumps(g.klasyfikuj(tekst), ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
