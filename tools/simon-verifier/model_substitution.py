#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Model-substitution SENSITIVITY test (nie proof).

Wg oceny hackerskiej 2026-10-05:
- `reference`, nie `verifier` (gdy honest == reference, dystans 0 jest tautologiczny).
- Exact tokenizer metadata check PRZED startem (chat_template, BOS, EOS, add_bos/eos).
- **Fixed sequence** do teacher-forcing (nie każdy model generuje swoją).
- Metryki: top-2 overlap, top-5 overlap, KL(ref||other), entropy diff, contain@1/@2.
- Random-token jako OSOBNY negative control.
- JSONL z surowymi danymi; AUROC + histogram overlap; brak targetu „AUROC≥0.9 jako claim".

Użycie:
  python3 model_substitution.py --ref http://127.0.0.1:18201 --sub http://127.0.0.1:18200 \
      --n 3 --tokens 20 --topk 5 --out /tmp/sub.jsonl
"""
from __future__ import annotations
import argparse, json, math, sys, urllib.request

def _post(base, path, body, timeout=900):
    req = urllib.request.Request(base.rstrip("/") + path, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)

def props(base):
    return _post(base, "/props", {}) if False else json.load(urllib.request.urlopen(base.rstrip("/") + "/props", timeout=20))

def tokenizer_ok(ref, sub):
    a, b = props(ref), props(sub)
    pola = ["chat_template", "bos_token", "eos_token", "add_bos_token", "add_eos_token"]
    for p in pola:
        if a.get(p) != b.get(p):
            return False, p
    return True, None

def _topk(krok, k):
    tops = {}
    if krok.get("id") is not None:
        tops[krok["id"]] = krok.get("logprob")
    for x in krok.get("top_logprobs", []) or []:
        tops[x["id"]] = x.get("logprob")
    return sorted(((i, lp) for i, lp in tops.items() if i is not None), key=lambda kv: -(kv[1] or -1e9))[:k]

def gen_reference(base, prompt_ids, n, k):
    """Greedy generacja referencji: zwraca (sequence, per_step_topk)."""
    d = _post(base, "/completion", {"prompt": prompt_ids, "n_predict": n, "temperature": 0,
                                    "top_k": 1, "seed": 42, "ignore_eos": True, "cache_prompt": False,
                                    "stream": False, "n_probs": k})
    seq, tops = [], []
    for s in d.get("completion_probabilities") or []:
        seq.append(s["id"]); tops.append(_topk(s, k))
    return seq, tops

def force_eval(base, prompt_ids, seq, k):
    """Teacher-forcing na STAŁEJ sekwencji: dla każdego kroku top-k przy kontekście prompt+seq[:t]."""
    out = []
    for t in range(len(seq)):
        d = _post(base, "/completion", {"prompt": list(prompt_ids) + list(seq[:t]), "n_predict": 1,
                                        "temperature": 0, "top_k": 1, "seed": 42, "n_probs": k,
                                        "cache_prompt": True, "ignore_eos": True, "stream": False})
        cp = (d.get("completion_probabilities") or [{}])[0]
        out.append(_topk(cp, k))
    return out

def _kl(ref, other):
    # przybliżone KL(ref||other) po unii top-k, z renormalizacją
    ids = set(i for i, _ in ref) | set(i for i, _ in other)
    def renorm(tops):
        d = {i: math.exp(lp) for i, lp in tops}
        s = sum(d.values()) or 1.0
        return {i: d.get(i, 0.0) / s for i in ids}
    p, q = renorm(ref), renorm(other)
    return sum(p[i] * math.log((p[i] + 1e-12) / (q[i] + 1e-12)) for i in ids)

def _entropy(tops):
    p = [math.exp(lp) for _, lp in tops]
    s = sum(p) or 1.0
    return -sum((x / s) * math.log((x / s) + 1e-12) for x in p)

def metryki(ref_t, other_t, k):
    r2 = set(i for i, _ in ref_t[:2]); o2 = set(i for i, _ in other_t[:2])
    r5 = set(i for i, _ in ref_t[:k]); o5 = set(i for i, _ in other_t[:k])
    return {
        "top2_overlap": len(r2 & o2) / max(1, len(r2 | o2)),
        "top5_overlap": len(r5 & o5) / max(1, len(r5 | o5)),
        "kl": _kl(ref_t, other_t),
        "entropy_ref": _entropy(ref_t),
        "entropy_other": _entropy(other_t),
        "contain1": 1 if (other_t and ref_t and other_t[0][0] == ref_t[0][0]) else 0,
        "contain2": 1 if (other_t and ref_t and other_t[0][0] in r2) else 0,
    }

def auroc(pos, neg):
    """pos=substitute scores (im niższy overlap tym bardziej 'pozytywny'), neg=honest."""
    if not pos or not neg:
        return float("nan")
    wins = 0.0
    for p in pos:
        for n in neg:
            wins += 1.0 if p < n else (0.5 if p == n else 0.0)   # niższy = bardziej podejrzany
    return wins / (len(pos) * len(neg))

PROMPTY = [
    "Explain in two sentences why a signature does not prove a computation.",
    "Write a Python function that returns the n-th Fibonacci number iteratively.",
    "Summarize the causes of the First World War in four sentences.",
]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ref", required=True)
    ap.add_argument("--sub", required=True)
    ap.add_argument("--n", type=int, default=3)
    ap.add_argument("--tokens", type=int, default=20)
    ap.add_argument("--topk", type=int, default=5)
    ap.add_argument("--out", default="/tmp/sub.jsonl")
    a = ap.parse_args()

    ok, pole = tokenizer_ok(a.ref, a.sub)
    print(f"tokenizer exact: {'OK' if ok else 'DIFF w '+str(pole)}")
    if not ok:
        print("STOP: tokenizery różne — nie mieszaj."); return 2

    f = open(a.out, "w", encoding="utf-8")
    honest_scores, sub_scores = [], []
    for pid in range(a.n):
        prompt = PROMPTY[pid % len(PROMPTY)]
        pids = _post(a.ref, "/tokenize", {"content": prompt})["tokens"]
        seq, ref_top = gen_reference(a.ref, pids, a.tokens, a.topk)
        honest_top = force_eval(a.ref, pids, seq, a.topk)      # ten sam model (reference)
        sub_top = force_eval(a.sub, pids, seq, a.topk)         # substitute (14B)
        # random-token control: losowe tokeny zamiast modelu
        import random; random.seed(pid)
        rand_tokens = [random.randint(0, 100000) for _ in seq]
        rand_top = force_eval(a.sub, pids, rand_tokens, a.topk)

        def agregat(tops, label):
            ms = [metryki(ref_top[t], tops[t], a.topk) for t in range(len(seq))]
            avg = {key: sum(m[key] for m in ms) / len(ms) for key in ms[0]}
            avg.update({"prompt_id": pid, "label": label, "n": len(seq)})
            f.write(json.dumps(avg, ensure_ascii=False) + "\n")
            return avg

        h = agregat(honest_top, "honest")
        s = agregat(sub_top, "substitute")
        r = agregat(rand_top, "random_control")
        honest_scores.append(h["top2_overlap"]); sub_scores.append(s["top2_overlap"])
        print(f"p{pid}: honest top2={h['top2_overlap']:.3f} kl={h['kl']:.4f} | "
              f"sub top2={s['top2_overlap']:.3f} kl={s['kl']:.4f} | "
              f"random top2={r['top2_overlap']:.3f}")

    f.close()
    print(f"\nAUROC(honest vs substitute) by top2_overlap = {auroc(sub_scores, honest_scores):.3f}")
    print(f"mean top2: honest={sum(honest_scores)/len(honest_scores):.3f}  "
          f"substitute={sum(sub_scores)/len(sub_scores):.3f}")
    print(f"JSONL: {a.out}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
