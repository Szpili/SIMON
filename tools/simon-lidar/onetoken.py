#!/usr/bin/env python3
"""Second layer of the verifier: single-token output distributions (after Bruckner, "One Token Is Enough",
arXiv:2607.10252). Ask trivial one-word questions many times at temperature 1.0; the answer distribution
is a fingerprint of checkpoint + quantization + sampler. Distance = mean Jensen-Shannon divergence over
the probe cells. Cheap: a few output tokens per query, no agent loop.

    onetoken.py run    --base URL/v1 --model NAME --label CLASS --n 30 --out ot.jsonl
    onetoken.py eval   --refs ot.jsonl            # split-sample genuine vs impostor distances
"""
import argparse, collections, itertools, json, math, re, sys, unicodedata, urllib.request
from concurrent.futures import ThreadPoolExecutor

ONLY = {"en": "Answer with one word only.", "pl": "Odpowiedz jednym s\u0142owem."}
TASKS = {
    "num100": {"en": "Name a random number between 1 and 100.", "pl": "Podaj losow\u0105 liczb\u0119 od 1 do 100."},
    "num10": {"en": "Name a random number between 1 and 10.", "pl": "Podaj losow\u0105 liczb\u0119 od 1 do 10."},
    "favnum": {"en": "What is your favorite number?", "pl": "Jaka jest Twoja ulubiona liczba?"},
    "letter": {"en": "Name a random letter of the alphabet.", "pl": "Podaj losow\u0105 liter\u0119 alfabetu."},
    "word": {"en": "Say a random word.", "pl": "Powiedz losowe s\u0142owo."},
    "color": {"en": "Name a random color.", "pl": "Podaj losowy kolor."},
    "favcolor": {"en": "What is your favorite color?", "pl": "Jaki jest Tw\u00f3j ulubiony kolor?"},
    "animal": {"en": "Name a random animal.", "pl": "Podaj losowe zwierz\u0119."},
    "city": {"en": "Name a random city.", "pl": "Podaj losowe miasto."},
    "coin": {"en": "Flip a coin. Heads or tails?", "pl": "Rzu\u0107 monet\u0105. Orze\u0142 czy reszka?"},
}
CELLS = [(t, l) for t in TASKS for l in ("en", "pl")]
MIN_VALID = 10  # the paper's floor for a usable cell


def ask(base, model, text):
    body = {"model": model, "messages": [{"role": "user", "content": text}], "max_tokens": 6,
            "temperature": 1.0, "top_p": 1.0}
    r = urllib.request.Request(base.rstrip("/") + "/chat/completions", data=json.dumps(body).encode(),
                               headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(r, timeout=300) as resp:
            return json.load(resp)["choices"][0]["message"].get("content") or ""
    except Exception as e:
        return "\0error:" + str(e)[:80]


def norm(s):
    """Unicode NFC, strip punctuation and quotes, case-fold, first word. '' = invalid."""
    if s.startswith("\0error"):
        return ""
    s = unicodedata.normalize("NFC", s).strip().casefold()
    w = re.split(r"\s+", re.sub(r"[^\w\s]", " ", s).strip())
    return w[0] if w and w[0] else ""


def collect(base, model, n, workers):
    jobs = [(c, f"{TASKS[c[0]][c[1]]} {ONLY[c[1]]}") for c in CELLS for _ in range(n)]
    with ThreadPoolExecutor(workers) as ex:
        answers = list(ex.map(lambda j: norm(ask(base, model, j[1])), jobs))
    cells = collections.defaultdict(list)
    for (c, _), a in zip(jobs, answers):
        cells[f"{c[0]}:{c[1]}"].append(a)
    return cells


def jsd(a, b):
    """Jensen-Shannon divergence (base 2) between two lists of answers ('' ignored)."""
    a, b = [x for x in a if x], [x for x in b if x]
    if len(a) < MIN_VALID or len(b) < MIN_VALID:
        return None
    pa, pb = collections.Counter(a), collections.Counter(b)
    d = 0.0
    for k in set(pa) | set(pb):
        p, q = pa[k] / len(a), pb[k] / len(b)
        m = (p + q) / 2
        d += 0.5 * (p * math.log2(p / m) if p else 0) + 0.5 * (q * math.log2(q / m) if q else 0)
    return d


def distance(fa, fb):
    ds = [x for c in fa if c in fb and (x := jsd(fa[c], fb[c])) is not None]
    return sum(ds) / len(ds) if ds else None


def halves(fp):
    """Split-sample: two disjoint halves of one fingerprint, for genuine (same-model) distances."""
    a, b = {}, {}
    for c, xs in fp.items():
        a[c], b[c] = xs[0::2], xs[1::2]
    return a, b


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run"); r.add_argument("--base", required=True); r.add_argument("--model", required=True)
    r.add_argument("--label", required=True); r.add_argument("--n", type=int, default=30)
    r.add_argument("--workers", type=int, default=2); r.add_argument("--out", required=True)
    e = sub.add_parser("eval"); e.add_argument("--refs", required=True, nargs="+")
    a = ap.parse_args()
    if a.cmd == "run":
        cells = collect(a.base, a.model, a.n, a.workers)
        valid = sum(1 for xs in cells.values() for x in xs if x) / max(1, sum(len(xs) for xs in cells.values()))
        with open(a.out, "a") as f:
            f.write(json.dumps({"label": a.label, "model": a.model, "base": a.base, "n": a.n, "cells": cells}) + "\n")
        print(f"{a.label}: {len(cells)} cells, valid {valid:.0%} -> {a.out}")
        for c in ("num100:en", "color:en", "coin:pl"):
            print(f"  {c}: {collections.Counter(cells[c]).most_common(4)}")
        return
    fps = [json.loads(l) for p in a.refs for l in open(p) if l.strip()]
    labels = [f["label"] for f in fps]
    print("genuine (split-sample, same fingerprint):")
    gen = []
    for f in fps:
        d = distance(*halves(f["cells"]))
        gen.append(d)
        print(f"  {f['label']}: {d:.4f}" if d is not None else f"  {f['label']}: too few valid answers")
    print("between fingerprints (same label = genuine, different label = impostor):")
    for (i, f), (j, g) in itertools.combinations(enumerate(fps), 2):
        d = distance(halves(f["cells"])[0], halves(g["cells"])[0])  # same sample size as the genuine split
        kind = "genuine " if labels[i] == labels[j] else "impostor"
        print(f"  {kind} {f['label']} vs {g['label']}: {d:.4f}" if d is not None else f"  {f['label']} vs {g['label']}: n/a")


if __name__ == "__main__":
    main()
