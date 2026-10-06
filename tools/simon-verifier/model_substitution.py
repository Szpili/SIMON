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
import argparse, json, math, sys, time, urllib.request, urllib.error

def _post(base, path, body, timeout=900, proby=4):
    # llama.cpp potrafi zwrócić 500 (np. "Content-only format") na pojedynczym żądaniu —
    # retry z backoffem, żeby długi run nie padał na jednym kroku.
    ostatni = None
    for k in range(proby):
        try:
            req = urllib.request.Request(base.rstrip("/") + path, data=json.dumps(body).encode(),
                                         headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            ostatni = e
            if e.code in (500, 502, 503, 504) and k < proby - 1:
                time.sleep(2 * (k + 1))
                continue
            raise
    raise ostatni

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
    "List the first ten prime numbers separated by commas.",
    "What is the capital of Australia? Answer in one word.",
    "Write a haiku about the sea.",
    "Translate 'good morning' into Polish, German, and Japanese.",
    "Explain photosynthesis to a ten-year-old in three sentences.",
    "Give three reasons to prefer local inference over a cloud API.",
    "Write a SQL query that selects the top 5 customers by total spend.",
    "What is 17 times 23 plus 41? Show the steps.",
    "Name the largest planet in the solar system and its diameter.",
    "Describe the difference between TCP and UDP in two sentences.",
    "Write a Rust function that reverses a string in place.",
    "List four causes of inflation in a market economy.",
    "Explain what a Merkle tree is in plain language.",
    "Compose a polite email declining a meeting invitation.",
    "What are the symptoms of dehydration in adults?",
    "Write the first stanza of a poem about autumn.",
    "Explain the difference between symmetric and asymmetric encryption.",
    "Give a two-sentence summary of the plot of Romeo and Juliet.",
    "How do you compute the median of a list of numbers?",
    "Name three countries in South America and their capitals.",
    "Describe a binary search algorithm in three steps.",
    "Write a short product description for a mechanical keyboard.",
    "What is the boiling point of water at sea level in Celsius?",
    "Explain why the sky is blue in two sentences.",
    "List five vegetables that grow well in a temperate climate.",
    "Write a one-paragraph story about a lost dog.",
    "Explain the difference between a process and a thread.",
    "Give three tips for writing clear technical documentation.",
    "What is the time complexity of quicksort in the average case?",
]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ref", required=True)
    ap.add_argument("--sub", required=True)
    # honest cross-device: ten sam model/GGUF/commit co --ref, ale inne urządzenie/offload.
    # Bez tego `honest == reference` jest tautologiczne (dystans 0).
    ap.add_argument("--honest", default=None, help="endpoint honest (domyślnie = --ref)")
    ap.add_argument("--n", type=int, default=3)
    ap.add_argument("--tokens", type=int, default=20)
    ap.add_argument("--topk", type=int, default=5)
    ap.add_argument("--out", default="/tmp/sub.jsonl")
    a = ap.parse_args()

    honest_ep = a.honest or a.ref
    ok1, p1 = tokenizer_ok(a.ref, a.sub)
    ok2, p2 = tokenizer_ok(a.ref, honest_ep)
    ok = ok1 and ok2
    pole = p1 or p2
    print(f"tokenizer exact: {'OK' if ok else 'DIFF w '+str(pole)}")
    if not ok:
        print("STOP: tokenizery różne — nie mieszaj."); return 2
    print(f"honest endpoint: {honest_ep} ({'cross-device' if honest_ep != a.ref else 'SAME as reference - tautologiczne'})")

    f = open(a.out, "w", encoding="utf-8")
    honest_scores, sub_scores = [], []
    honest_c2, sub_c2 = [], []   # contain2 = shipowana metryka m3::ocena
    pominięte = 0
    for pid in range(a.n):
        try:
            prompt = PROMPTY[pid % len(PROMPTY)]
            pids = _post(a.ref, "/tokenize", {"content": prompt})["tokens"]
            seq, _ = gen_reference(a.ref, pids, a.tokens, a.topk)
            # REGIME-MATCHED: ref_top przez force_eval (cache_prompt:True, 1-token), tak jak ramiona.
            # (fable 2026-10-06: wcześniej ref_top był batched n_predict=20 → mieszał device z prefill-vs-cache.)
            ref_top = force_eval(a.ref, pids, seq, a.topk)
            honest_top = force_eval(honest_ep, pids, seq, a.topk)  # honest (cross-device albo ref)
            sub_top = force_eval(a.sub, pids, seq, a.topk)         # substitute
            # random-token control: losowe tokeny zamiast modelu
            import random; random.seed(pid)
            rand_tokens = [random.randint(0, 100000) for _ in seq]
            rand_top = force_eval(a.sub, pids, rand_tokens, a.topk)
        except Exception as e:  # noqa: BLE001 — pojedynczy prompt/server-500 nie zabija runu
            pominięte += 1
            print(f"p{pid}: SKIP ({str(e)[:80]})")
            continue

        import hashlib
        seq_hash = hashlib.sha256(",".join(map(str, seq)).encode()).hexdigest()[:16]

        def agregat(tops, label):
            ms = [metryki(ref_top[t], tops[t], a.topk) for t in range(len(seq))]
            avg = {key: sum(m[key] for m in ms) / len(ms) for key in ms[0]}
            avg.update({"prompt_id": pid, "label": label, "n": len(seq), "seq_hash": seq_hash})
            f.write(json.dumps(avg, ensure_ascii=False) + "\n")
            return avg

        h = agregat(honest_top, "honest_cross_device" if honest_ep != a.ref else "honest_same_reference")
        s = agregat(sub_top, "substitute")
        r = agregat(rand_top, "random_control")
        honest_scores.append(h["top2_overlap"]); sub_scores.append(s["top2_overlap"])
        honest_c2.append(h["contain2"]); sub_c2.append(s["contain2"])
        print(f"p{pid}: honest({h['label']}) top2={h['top2_overlap']:.3f} kl={h['kl']:.4f} | "
              f"sub top2={s['top2_overlap']:.3f} kl={s['kl']:.4f} | "
              f"random top2={r['top2_overlap']:.3f}")

    f.close()
    print(f"pominięte prompty: {pominięte}")
    # SHIPOWANA metryka to contain2 (m3::ocena), nie top2_overlap (fable 2026-10-06).
    print(f"\nAUROC by top2_overlap          = {auroc(sub_scores, honest_scores):.3f}")
    print(f"AUROC by contain2 (SHIPOWANE)  = {auroc(sub_c2, honest_c2):.3f}")
    print(f"mean top2: honest={sum(honest_scores)/len(honest_scores):.3f}  "
          f"substitute={sum(sub_scores)/len(sub_scores):.3f}  |  "
          f"mean contain2: honest={sum(honest_c2)/max(1,len(honest_c2)):.3f} "
          f"substitute={sum(sub_c2)/max(1,len(sub_c2)):.3f}")
    print(f"JSONL: {a.out}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
