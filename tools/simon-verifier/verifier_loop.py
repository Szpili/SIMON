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
import argparse, json, re, sys, urllib.parse, urllib.request


def _validate_endpoint(endpoint: str) -> str:
    """Waliduje poprawność URL/multiaddr endpointu, odrzucając złośliwe znaki."""
    if not endpoint or not isinstance(endpoint, str):
        raise ValueError("Pusty endpoint")
    if re.search(r"[\s\x00-\x1f\x7f;<>\'\"\`\|\$\&\(\)]", endpoint):
        raise ValueError(f"Nieprawidłowe znaki w adresie endpointu: {endpoint!r}")
    parsed = urllib.parse.urlparse(endpoint)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        raise ValueError(f"Nieobsługiwany format adresu endpointu (wymagany http:// lub https://): {endpoint!r}")
    return endpoint


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


def run_selfcheck() -> int:
    """Weryfikuje logikę _topk i format kroków bez potrzeby odpytywania llama-server."""
    mock_cp = {
        "id": 100,
        "logprob": -0.05,
        "top_logprobs": [
            {"id": 101, "logprob": -1.2},
            {"id": 102, "logprob": -3.4},
        ]
    }
    top = _topk(mock_cp)
    assert len(top) == 3, f"Oczekiwano 3 elementów w topk, otrzymano {len(top)}"
    assert top[0] == (100, -0.05)
    kroki = [{"indeks": 0, "node_token": 100, "topk": top}]
    audyt_str = json.dumps({"kroki": kroki})
    parsed = json.loads(audyt_str)
    assert "kroki" in parsed and len(parsed["kroki"]) == 1
    print("verifier_loop: selfcheck OK", file=sys.stderr)
    return 0


def dry_run(prompt_ids: list, output_ids: list, k: int = 5) -> list:
    """Generuje symulowany audyt kroki z syntetycznymi prawdopodobieństwami."""
    kroki = []
    for t, tok in enumerate(output_ids):
        simulated_topk = [(int(tok), -0.01)] + [(int(tok) + i + 1, -2.0 - i) for i in range(k - 1)]
        kroki.append({"indeks": t, "node_token": int(tok), "topk": simulated_topk})
    return kroki


def main() -> int:
    ap = argparse.ArgumentParser(description="Realny verifier-loop M3.")
    ap.add_argument("zadanie", nargs="?", default=None, help="JSON z prompt_token_ids i output_token_ids")
    ap.add_argument("--endpoint", "--node", dest="endpoint", default="http://127.0.0.1:18101",
                    help="Endpoint llama-server lub adres węzła")
    ap.add_argument("--topk", type=int, default=5)
    ap.add_argument("--out", default="-", help="- = stdout")
    ap.add_argument("--selfcheck", action="store_true", help="Uruchom wewnętrzny self-check i wyjdź")
    ap.add_argument("--dry-run", action="store_true", help="Wygeneruj symulowany audyt bez łączenia z endpointem")
    a = ap.parse_args()

    if a.selfcheck:
        return run_selfcheck()

    try:
        _validate_endpoint(a.endpoint)
    except ValueError as e:
        print(f"Błąd walidacji endpointu: {e}", file=sys.stderr)
        return 1

    if a.dry_run:
        if a.zadanie:
            z = json.load(open(a.zadanie, encoding="utf-8"))
            prompt_ids = z.get("prompt_token_ids", [])
            output_ids = z.get("output_token_ids", [])
        else:
            prompt_ids = [1, 2, 3]
            output_ids = [4, 5, 6]
        kroki = dry_run(prompt_ids, output_ids, a.topk)
        audyt = json.dumps({"kroki": kroki}, ensure_ascii=False)
        if a.out == "-":
            print(audyt)
        else:
            open(a.out, "w", encoding="utf-8").write(audyt)
            print(f"zapisano {a.out} ({len(kroki)} kroków dry-run)", file=sys.stderr)
        return 0

    if not a.zadanie:
        ap.error("Wymagany argument 'zadanie' (ścieżka do pliku JSON) lub flaga --selfcheck / --dry-run")

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
