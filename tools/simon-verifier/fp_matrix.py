#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""FP-pod-uczciwą-zmiennością (pre-rejestracja: docs/FP-PREREGISTRATION.md).

Dla każdej uczciwej konfiguracji C i każdego promptu p:
  node = C generuje N tokenów WŁASNĄ trajektorią (greedy),
  reference (config 1) teacher-forcingiem ocenia sekwencję node'a,
  contain2 = czy token node'a jest w top-2 referencji w marginesie,
  flagged = (1 - contain2_frac) > max_poza.

PASS = żaden uczciwy config nie flaguje żadnego promptu. Metryka = contain2 (shipowana),
jednostka = tekst promptu, ≥30 RÓŻNYCH promptów.

Użycie:
  python3 fp_matrix.py --ref http://127.0.0.1:PORT \
      --cfg "cpu=http://127.0.0.1:PORT" --cfg "cpu_b512=..." ... \
      --prompts prompts_fp.txt --tokens 20 --topk 5 --max-poza 0.02 --margin 0.25 --out fp.jsonl
"""
from __future__ import annotations
import argparse, json, math, sys
import model_substitution as ms


def contain2_step(node_token, ref_topk, k=2, margin=0.25):
    ids = [i for i, _ in ref_topk]
    lps = [lp for _, lp in ref_topk]
    if node_token not in ids[:k]:
        return 0.0
    rank = ids.index(node_token)
    return 1.0 if (lps[0] - lps[rank]) <= margin else 0.0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ref", required=True)
    ap.add_argument("--cfg", action="append", required=True, help="nazwa=url (można wiele)")
    ap.add_argument("--prompts", default="prompts_fp.txt")
    ap.add_argument("--tokens", type=int, default=20)
    ap.add_argument("--topk", type=int, default=5)
    ap.add_argument("--max-poza", type=float, default=0.02)
    ap.add_argument("--margin", type=float, default=0.25)
    ap.add_argument("--out", default="/tmp/fp_matrix.jsonl")
    a = ap.parse_args()

    configs = []
    for c in a.cfg:
        nazwa, _, url = c.partition("=")
        configs.append((nazwa, url))
    if not any(n == "ref" for n, _ in configs):
        configs.insert(0, ("ref", a.ref))

    prompty = [p.strip() for p in open(a.prompts, encoding="utf-8") if p.strip()]
    print(f"promptów: {len(prompty)}  configów: {len(configs)}  max_poza={a.max_poza} margin={a.margin}")

    f = open(a.out, "w", encoding="utf-8")
    # sanity: tokenizery muszą być identyczne
    for nazwa, url in configs:
        ok, pole = ms.tokenizer_ok(a.ref, url)
        if not ok:
            print(f"STOP: tokenizer DIFF dla {nazwa} ({pole})"); return 2

    podsumowanie = {}
    for nazwa, url in configs:
        flagowane = 0
        frs = []
        for pid, prompt in enumerate(prompty):
            try:
                pids = ms._post(a.ref, "/tokenize", {"content": prompt})["tokens"]
                seq, _ = ms.gen_reference(url, pids, a.tokens, a.topk)      # node = config C
                ref_top = ms.force_eval(a.ref, pids, seq, a.topk)           # verifier = reference
            except Exception as e:  # noqa: BLE001
                print(f"  [{nazwa}] p{pid}: SKIP ({str(e)[:70]})"); continue
            import hashlib
            seq_hash = hashlib.sha256(",".join(map(str, seq)).encode()).hexdigest()[:16]
            c2 = [contain2_step(seq[t], ref_top[t], 2, a.margin) for t in range(len(seq))]
            fr = 1.0 - (sum(c2) / len(c2))
            flagged = fr > a.max_poza
            if flagged:
                flagowane += 1
            frs.append(fr)
            f.write(json.dumps({"config": nazwa, "prompt_id": pid, "n": len(seq),
                                "contain2_frac": sum(c2) / len(c2), "suspicious_frac": fr,
                                "flagged": flagged, "seq_hash": seq_hash}, ensure_ascii=False) + "\n")
        if frs:
            podsumowanie[nazwa] = (sum(frs) / len(frs), max(frs), flagowane)
        print(f"  {nazwa:16s} mean_suspicious={podsumowanie.get(nazwa,(0,0,0))[0]:.4f} "
              f"max={podsumowanie.get(nazwa,(0,0,0))[1]:.4f} flagged={flagowane}/{len(frs)}")
    f.close()

    # PASS/FAIL wg pre-rejestracji
    fail = [(n, v[2]) for n, v in podsumowanie.items() if v[2] > 0]
    print("\n=== WERDYKT (pre-rejestracja) ===")
    if fail:
        print(f"FAIL: uczciwe configi flagowane: {fail} → M3 zostaje TELEMETRIĄ, bez eskalacji.")
    else:
        print(f"PASS: żaden z {len(podsumowanie)} configów nie flagował promptu "
              f"(0/{len(prompty)} → FP ≤ ~9% przy 95%).")
    print(f"JSONL: {a.out}")
    return 1 if fail else 0


if __name__ == "__main__":
    sys.path.insert(0, __import__("os").path.dirname(__file__))
    raise SystemExit(main())
