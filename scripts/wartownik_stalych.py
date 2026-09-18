#!/usr/bin/env python3
"""Szuka sygnatury błędu z 2026-09-18: liczby tokenów dzielonej przez STAŁĄ.

`tokens_out / 65.0 * 1000` udawało pomiar przepustowości. Wykrywamy dzielenie
liczby tokenów przez literał, POMIJAJĄC:
  - komentarze (opis incydentu musi móc zostać w kodzie),
  - dzielenie przez 1000 (przeliczenie milisekund na sekundy),
  - dzielenie przez ZMIENNĄ (np. zmierzony `gen_ms`) — to jest pomiar.

Zwraca 1, gdy coś znajdzie.
"""
import pathlib
import re
import sys

# `tokens...` (jakkolwiek nazwane) dzielone przez literał liczbowy
WZORZEC = re.compile(r"tokens?[_a-z]*\s*(?:as\s+f64\s*)?/\s*([0-9]+(?:\.[0-9]+)?)")
DOZWOLONE_DZIELNIKI = {"1000", "1000.0"}

korzen = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "crates")
trafienia = []

for plik in korzen.rglob("*.rs"):
    if "target" in plik.parts:
        continue
    for nr, linia in enumerate(plik.read_text(encoding="utf-8").splitlines(), 1):
        if linia.lstrip().startswith("//"):
            continue
        for dzielnik in WZORZEC.findall(linia):
            if dzielnik in DOZWOLONE_DZIELNIKI:
                continue
            trafienia.append(f"{plik}:{nr}: dzielenie tokenów przez stałą {dzielnik} -> {linia.strip()}")

if trafienia:
    print("BŁĄD: liczba tokenów dzielona przez stałą — to sygnatura pomiaru, którego nie ma:")
    for t in trafienia:
        print("  " + t)
    sys.exit(1)

print(f"wartownik stałych: czysto ({korzen})")
