#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Realny verifier-loop M3.

Podąża trajektorią node'a (teacher-forcing po tokenach) i zbiera top-k verifiera
krok po kroku. Wynik (audyt) ocenia Rust: `simon audyt <plik>` → `m3::ocena` +
`m3::decyzja` (Pass / SoftFail bez slasha / HardFail).

Dlaczego llama.cpp: potrzebujemy `prompt` jako TABLICY tokenów i `n_probs` per krok.
OpenAI-compatible vLLM tego nie wystawia. Dla joba produkcyjnego verifier to osobny
llama.cpp z tym samym modelem co node.

Wejście: zadanie JSON `{prompt_token_ids, output_token_ids, ...}` (to samo, co
commit-uje M1 w receipcie). Endpoint: `http://127.0.0.1:<port>` (llama-server).

Użycie:
  python3 verifier_loop.py zadanie.json --endpoint http://127.0.0.1:18101 --out audyt.json
  simon audyt audyt.json
"""
from __future__ import annotations
import argparse, json, sys, urllib.request


def _post(base: str, path: str, body: dict, timeout: int = 900) -> dict:
    req = urllib.request.Request(base.rstrip("/") + path, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)


def _topk(krok: dict) -> list:
    tops = {}
    if krok.get("id") is not None:
        tops[krok["id"]] = krok.get("logprob")
    for a in krok.get("top_logprobs", []) or []:
        tops[a["id"]] = a.get("logprob")
    return sorted(((k, v) for k, v in tops.items() if k is not None),
                  key=lambda kv: -(kv[1] if kv[1] is not None else -1e9))


def petla(endpoint: str, prompt_ids: list, output_ids: list, k: int = 5) -> list:
    kroki = []
    for t, tok in enumerate(output_ids):
        ctx = list(prompt_ids) + list(output_ids[:t])
        d = _post(endpoint, "/completion", {
            "prompt": ctx, "n_predict": 1, "temperature": 0, "top_k": 1, "seed": 42,
            "n_probs": k, "cache_prompt": True, "ignore_eos": True, "stream": False,
        })
        cp = (d.get("completion_probabilities") or [{}])[0]
        kroki.append({"indeks": t, "node_token": int(tok), "topk": _topk(cp)})
    return kroki


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("zadanie", help="JSON z prompt_token_ids i output_token_ids")
    ap.add_argument("--endpoint", default="http://127.0.0.1:18101")
    ap.add_argument("--topk", type=int, default=5)
    ap.add_argument("--out", default="-", help="- = stdout")
    a = ap.parse_args()

    z = json.load(open(a.zadanie, encoding="utf-8"))
    kroki = petla(a.endpoint, z["prompt_token_ids"], z["output_token_ids"], a.topk)
    audyt = json.dumps({"kroki": kroki}, ensure_ascii=False)
    if a.out == "-":
        print(audyt)
    else:
        open(a.out, "w", encoding="utf-8").write(audyt)
        print(f"zapisano {a.out} ({len(kroki)} kroków)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
