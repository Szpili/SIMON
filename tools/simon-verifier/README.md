# SIMON — verifier-loop (M3)

Inferencję robi strona zewnętrzna (llama.cpp), **decyzję liczy Rust** (`simon audyt`).

## Jak działa

1. `verifier_loop.py` czyta zadanie (`prompt_token_ids`, `output_token_ids` — to samo, co
   commit-uje M1 w receipcie) i **podąża trajektorią node'a**: dla każdego kroku `t` pyta
   llama.cpp `/completion` o kontekst `prompt + output[0..t]` z `n_probs=K` i zbiera top-k
   verifiera.
2. `simon audyt audyt.json` liczy containment (`m3::ocena`) i decyzję (`m3::decyzja`):
   `Pass` / `SoftFail` (eskalacja **BEZ slasha**) / `HardFail` (slash) / `BrakPodstaw`.

## Dlaczego llama.cpp, nie vLLM

Potrzebny `prompt` jako **tablica tokenów** i `n_probs` per krok. OpenAI-compatible vLLM tego
nie wystawia. Verifier to osobny llama.cpp z tym samym modelem co node.

## Dlaczego top-k, nie równość tokenów

E0: `temp=0` greedy **nie jest przenośny CPU↔GPU** (dywergencja od ~3 tokenów; kontrola
GPU×GPU bit-identyczna). Kalibracja: dywergencja to zawsze spór **top-1 vs top-2**; gdy
verifier podąża trajektorią node'a, token node'a jest w **top-2 w 150/150 kroków**. Stąd
polityka `k=2`, `max_margin=0,25`. **Nigdy nie slasujemy za pojedynczy token.**

## Użycie

```sh
python3 verifier_loop.py zadanie.json --endpoint http://127.0.0.1:18101 --out audyt.json
simon audyt audyt.json --poziom 1
```

Fakty: `~/brain/facts/simon-e0-fp-divergence-2026-10-04.md`,
`~/brain/facts/simon-m3-topk-kalibracja-2026-10-04.md`.
