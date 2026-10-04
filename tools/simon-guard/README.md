# SIMON — mały guard (CPU)

Brama wejścia dla **małych node'ów** (słaby sprzęt, bez GPU): moderacja treści + filtr PII/sekretów.
Powstała, bo „u Joe bielik nie wejdzie" — guard robimy sami, tak by działał na CPU.

## Warstwy

1. **Reguły** (`pii_pl.py`, deterministyczne, sumy kontrolne): PII strukturalne
   (PESEL/NIP/REGON/IBAN/TEL/EMAIL/…) i SEKRETY (KLUCZ_API/TOKEN/HASLO/CONNSTR/…). ~0,1 ms.
2. **Model** (`speakleash/Bielik-Guard-0.5B-v1.1`, apache-2.0): toksyczność w 5 kategoriach
   (hate / vulgar / sex / crime / self-harm). ~150 ms na CPU.

## Pomiar (51 przypadków: PII 12 / SEKRET 12 / INJECTION 12 / CZYSTE 15, CPU)

| warstwa | PII | SEKRET | INJECTION | CZYSTE FP | latencja |
|---|---|---|---|---|---|
| reguły | 7/12 | 11/12 | ~0 | 1/15 | 0,10 ms |
| + model 0.5B | (reguły) | (reguły) | 3/12 (przypadek) | 1/15 | 149,6 ms |
| + model 0.1B | (reguły) | (reguły) | 1/12 | 3/15 | 46,3 ms |

**0.5B = sweet spot.** Reguły łapią strukturalne PII i 11/12 sekretów; nazwiska/adresy i obfuskacja → nie.

## Czego NIE robi (uczciwie)

**Nie łapie prompt injection** (0–3/12 = przypadek). To warstwa semantyczna (LLM 11B/agent), nie
guard treści. Nie obiecywać „injection rozwiązany".

**RULES #1:** guard widzi treść ⇒ kto hostuje guard, ten widzi prompt. Guard po stronie klienta
i node'a, opt-in per zlecenie. **Nie** wpinamy guarda w receipt (to moderacja, nie dowód wykonania).

## Użycie

```sh
python3 guard_small.py "tekst"
echo "tekst" | python3 guard_small.py --stdin
python3 guard_small.py --serve --port 19301      # POST /guard {"text": "..."}
python3 guard_small.py --bench /sciezka/test-bielik-guard.py
```

Modele pobierz raz: `hf download speakleash/Bielik-Guard-0.5B-v1.1 --local-dir ~/.cache/bielik-guard/0.5B`
(domyślny `--dir`). Zależności: `transformers`, `torch` (CPU).

## Serwis

`~/.config/systemd/user/simon-guard-small.service` → `127.0.0.1:19301` (loopback),
`POST /guard` → `{ryzyko, pii, sekret, toksycznosc, kategorie, ms_reguly, ms_model}`.

## Dalej

- Pakiet offline (ONNX zamiast torch) dla naprawdę małych node'ów;
- wpięcie wywołania guarda w harness (klient przed wysłaniem, node przed liczeniem).
