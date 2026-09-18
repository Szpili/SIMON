#!/usr/bin/env python3
"""D2: analiza. Dwa ROZNE pytania, liczone osobno.

1) POWTARZALNOSC WEWNETRZNA: czy dana sciezka jest powtarzalna sama ze soba?
2) ZGODNOSC MIEDZY SILNIKAMI: czy verifier na innym silniku odtworzy wynik?

Wynik "kazda sciezka stabilna, ale A != B" jest ISTOTNY: wtedy problemem nie
jest niedeterminizm, tylko DETERMINISTYCZNA ROZNICA IMPLEMENTACJI.
"""
import json, math, statistics, sys, os

KAT = sys.argv[1] if len(sys.argv) > 1 else "."
ramiona = {}
for r in "ABCDE":
    p = os.path.join(KAT, f"d2_{r}.json")
    if os.path.exists(p):
        ramiona[r] = json.load(open(p))

def stat(a, b):
    pary = [(x, y) for x, y in zip(a, b) if not (math.isnan(x) or math.isnan(y))]
    if not pary: return None
    d = [abs(x - y) for x, y in pary]
    ident = sum(1 for x in d if x == 0.0)
    d_s = sorted(d)
    return {"n": len(d), "bit_identyczne_%": 100*ident/len(d),
            "mediana": statistics.median(d), "p99": d_s[min(len(d)-1, int(0.99*len(d)))],
            "maks": max(d)}

print("=" * 72)
print("1) POWTARZALNOSC WEWNETRZNA (przebieg 1 vs kolejne, w obrebie ramienia)")
print("=" * 72)
print(f"{'ramie':<6}{'opis':<38}{'bit-ident':>11}{'maks delta':>13}")
for r, d in ramiona.items():
    pr = d["przebiegi"]
    if len(pr) < 2: continue
    wyniki = [stat(pr[0], p) for p in pr[1:]]
    wyniki = [w for w in wyniki if w]
    bi = min(w["bit_identyczne_%"] for w in wyniki)
    mx = max(w["maks"] for w in wyniki)
    print(f"{r:<6}{d['opis'][:37]:<38}{bi:>10.2f}%{mx:>13.3e}")

print()
print("=" * 72)
print("2) ZGODNOSC MIEDZY SILNIKAMI (pierwszy przebieg kazdego ramienia)")
print("=" * 72)
pary = [("A","B"),("A","C"),("A","E"),("B","C"),("B","E"),("C","D"),("C","E")]
print(f"{'para':<8}{'bit-ident':>11}{'mediana':>13}{'p99':>13}{'maks':>13}")
for x, y in pary:
    if x in ramiona and y in ramiona:
        s = stat(ramiona[x]["przebiegi"][0], ramiona[y]["przebiegi"][0])
        if s:
            print(f"{x}-{y:<6}{s['bit_identyczne_%']:>10.2f}%{s['mediana']:>13.3e}"
                  f"{s['p99']:>13.3e}{s['maks']:>13.3e}")

wynik = {"ramiona": {r: d["opis"] for r, d in ramiona.items()},
         "wewnetrzna": {r: [stat(d["przebiegi"][0], p) for p in d["przebiegi"][1:]]
                        for r, d in ramiona.items() if len(d["przebiegi"]) > 1},
         "miedzy_silnikami": {f"{x}-{y}": stat(ramiona[x]["przebiegi"][0], ramiona[y]["przebiegi"][0])
                              for x, y in pary if x in ramiona and y in ramiona}}
json.dump(wynik, open(os.path.join(KAT, "d2_analiza.json"), "w"), indent=2)
print(f"\nzapisano {os.path.join(KAT,'d2_analiza.json')}")
