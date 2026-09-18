#!/usr/bin/env python3
"""D2: izolacja zrodla dryfu. Jedno ramie na uruchomienie (osobny proces,
bo VLLM_BATCH_INVARIANT musi byc ustawione przed importem).

Obserwabla: prompt_logprobs na sciezce PREFILL - logprob faktycznego tokenu
na kazdej pozycji promptu. Te sama wielkosc liczymy z logitow transformers.
KLUCZOWE: obu stosom podajemy TE SAME input_ids (nie tekst), wiec tokenizacja
nie jest zmienna.

Uzycie: d2_arm.py <A|B|C|D|E> <powtorzen>
"""
import hashlib, json, os, sys

KAT_WY = os.environ.get("D2_OUT", "/work")
os.makedirs(KAT_WY, exist_ok=True)
RAMIE = sys.argv[1]
POWT = int(sys.argv[2]) if len(sys.argv) > 2 else 3
if RAMIE == "E":
    os.environ["VLLM_BATCH_INVARIANT"] = "1"
os.environ.setdefault("VLLM_ENABLE_V1_MULTIPROCESSING", "0")

MODEL = "Qwen/Qwen2.5-1.5B-Instruct"
TEKST = ("Weryfikacja obliczen w sieci rozproszonej wymaga, zeby wykonawca zobowiazal sie "
         "do sladu wykonania, zanim pozna punkty kontroli. Inaczej moze dobrac, co pokaze. " * 4)

from transformers import AutoTokenizer
tok = AutoTokenizer.from_pretrained(MODEL)
IDS = tok(TEKST)["input_ids"]          # USTALONE input_ids dla WSZYSTKICH ramion
print(f"[{RAMIE}] tokenow: {len(IDS)}", flush=True)

def zapisz(lp_przebiegi, opis):
    json.dump({"ramie": RAMIE, "opis": opis, "model": MODEL, "tokenow": len(IDS),
               "odcisk_input_ids": hashlib.sha256(json.dumps(IDS).encode()).hexdigest()[:16], "przebiegi": lp_przebiegi},
              open(os.path.join(KAT_WY, f"d2_{RAMIE}.json"), "w"))
    print(f"[{RAMIE}] zapisano {KAT_WY}/d2_{RAMIE}.json", flush=True)

if RAMIE == "A":
    import torch
    from transformers import AutoModelForCausalLM
    IMPL = os.environ.get("D2_ATTN", "eager")
    m = AutoModelForCausalLM.from_pretrained(MODEL, dtype=torch.float16,
                                             attn_implementation=IMPL).to("cuda").eval()
    ids = torch.tensor([IDS], device="cuda")
    przebiegi = []
    with torch.no_grad():
        for i in range(POWT):
            out = m(ids).logits[0].float()                 # [T, V]
            lp = torch.log_softmax(out, dim=-1)
            # logprob FAKTYCZNEGO nastepnego tokenu — to samo, co daje prompt_logprobs
            wart = [float(lp[t, IDS[t+1]]) for t in range(len(IDS)-1)]
            przebiegi.append(wart)
            print(f"[A] przebieg {i+1}/{POWT}", flush=True)
    zapisz(przebiegi, f"transformers attn={IMPL}, batch=1, fp16")

else:
    from vllm import LLM, SamplingParams
    kw = dict(model=MODEL, dtype="float16", gpu_memory_utilization=0.55, seed=0)
    if RAMIE == "B":
        kw.update(enforce_eager=True, max_num_seqs=1)
        opis = "vLLM eager, max_num_seqs=1"
    elif RAMIE == "C":
        opis = "vLLM domyslny, pojedynczy request"
    elif RAMIE == "D":
        opis = "vLLM domyslny, ZMIENNY co-batch"
    elif RAMIE == "E":
        opis = "vLLM VLLM_BATCH_INVARIANT=1"
    llm = LLM(**kw)
    sp = SamplingParams(temperature=0.0, max_tokens=1, prompt_logprobs=0, seed=0)

    przebiegi = []
    for i in range(POWT):
        if RAMIE == "D":
            # nasze zlecenie w srodku zmiennego wsadu
            wypelniacze = [{"prompt_token_ids": IDS[: 20 + 7*i + 3*j]} for j in range(1 + i)]
            wsad = wypelniacze[: (i % 2) + 1] + [{"prompt_token_ids": IDS}] + wypelniacze[: i % 3]
        else:
            wsad = [{"prompt_token_ids": IDS}]
        wyn = llm.generate(wsad, sp)
        nasz = [o for o in wyn if list(o.prompt_token_ids) == list(IDS)][0]
        wart = []
        for poz, d in enumerate(nasz.prompt_logprobs or []):
            if d is None:   # pierwsza pozycja nie ma logprobu
                continue
            tid = IDS[poz]
            wart.append(float(d[tid].logprob) if tid in d else float("nan"))
        przebiegi.append(wart)
        print(f"[{RAMIE}] przebieg {i+1}/{POWT} | pozycji: {len(wart)}", flush=True)
    zapisz(przebiegi, opis)
