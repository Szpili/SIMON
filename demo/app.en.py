"""SIMON — demo: zleć pracę obcemu węzłowi i NIE UWIERZ mu na słowo.

Cała teza projektu w jednym ekranie: wynik przychodzi z maszyny, której nie
kontrolujesz, razem z podpisanym receiptem — a Ty sprawdzasz ten receipt sam,
łącznie z próbą podrobienia go na Twoich oczach.

Demo woła prawdziwe `simon` CLI. Nic tu nie jest zasymulowane: jeśli węzeł nie
odpowiada, demo mówi, że nie odpowiada, zamiast pokazywać ładny wynik z puszki.
"""
import json
import os
import pathlib
import shutil
import subprocess

import streamlit as st

import limity

# Ścieżka liczona od pliku, nie od katalogu uruchomienia — streamlit bywa
# odpalany z dowolnego miejsca i "./target/..." wtedy nie istnieje.
DOMYSLNA = pathlib.Path(__file__).resolve().parent.parent / "target" / "release" / "simon"
SIMON = os.environ.get("SIMON_BIN") or shutil.which("simon") or str(DOMYSLNA)
NODE = os.environ.get("SIMON_NODE", "")

# Tryb publiczny: demo wystawione w internet. Zmienia trzy rzeczy, wszystkie
# dlatego, ze po drugiej stronie jest ktokolwiek, a nie my.
PUBLICZNY = os.environ.get("SIMON_PUBLIC") == "1"
LIMIT_NA_GODZINE = int(os.environ.get("SIMON_LIMIT_H", "40"))
LICZNIK = pathlib.Path(os.environ.get("SIMON_LICZNIK", "/tmp/simon-demo-licznik.json"))
# M5.2: metrycznik wykonanej pracy. NIE portfel i NIE saldo.
REJESTR = os.environ.get("SIMON_REJESTR", "")


def wczytaj_wezly() -> list[dict]:
    """Lista węzłów, z których WOLNO wybierać.

    W trybie publicznym to bramka bezpieczeństwa, nie wygoda: gdyby adres węzła
    pochodził od odwiedzającego, kazałby naszemu serwerowi łączyć się z dowolnym
    miejscem w sieci. Dlatego wybór jest zawsze z tej listy.
    """
    plik = os.environ.get("SIMON_NODES")
    if plik and pathlib.Path(plik).is_file():
        try:
            dane = json.loads(pathlib.Path(plik).read_text())
            wezly = [w for w in dane if w.get("adres")]
            if wezly:
                return wezly
        except (OSError, ValueError):
            # Zepsuta lista nie może wywalić demo — spadamy na pojedynczy węzeł.
            st.warning("cannot read the node list — using the default")
    if NODE:
        return [{"nazwa": "default node", "adres": NODE,
                 "model": os.environ.get("SIMON_MODEL", "qwen3.8-27b"), "opis": ""}]
    return []


WEZLY = wczytaj_wezly()


st.set_page_config(page_title="SIMON", page_icon="🔏", layout="wide")


def do_pliku(tresc: str) -> str:
    """Zapisuje tekst do pliku tymczasowego — --expect-output czyta z pliku,
    bo stdin jest juz zajety przez receipt."""
    import tempfile
    f = tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8")
    f.write(tresc)
    f.close()
    return f.name


def wywolaj(args: list[str], wejscie: str | None = None, limit_s: int = 300) -> dict:
    """Uruchamia CLI i wyciąga JSON. Stdout to wynik, stderr to dziennik —
    dlatego szukamy JSON-a od końca, a nie ufamy, że jest dokładnie jedną linią."""
    try:
        p = subprocess.run([SIMON, *args], input=wejscie, capture_output=True,
                           text=True, timeout=limit_s)
    except FileNotFoundError:
        return {"ok": False, "powod": f"binary not found: {SIMON}"}
    except subprocess.TimeoutExpired:
        return {"ok": False, "powod": f"node did not respond within {limit_s}s"}
    for linia in reversed((p.stdout or "").strip().splitlines()):
        try:
            return json.loads(linia)
        except json.JSONDecodeError:
            continue
    return {"ok": False, "powod": (p.stderr or "no response").strip()[-400:]}


with st.sidebar:
    st.header("Node")
    if PUBLICZNY:
        # Wybór TYLKO z listy — adres nigdy nie pochodzi od odwiedzającego.
        if WEZLY:
            wybrany = st.radio("Assign the job to", WEZLY,
                               format_func=lambda w: w["nazwa"])
            node, model = wybrany["adres"], wybrany["model"]
            if wybrany.get("opis"):
                st.caption(wybrany["opis"])
        else:
            node, model = "", ""
        max_tokens = st.slider("Token limit", 20, 200, 80, step=20)
        st.caption(f"public demo — limit {LIMIT_NA_GODZINE} jobs/hour")
    else:
        node = st.text_input("Node address (multiaddr)", NODE,
                             placeholder="/ip4/1.2.3.4/tcp/9001/p2p/12D3Koo...")
        model = st.text_input("Model", "qwen3.8-27b")
        max_tokens = st.slider("Token limit", 20, 400, 80, step=20)
        st.caption(f"binary: `{SIMON}`")

st.title("SIMON")
st.markdown(
    "You are assigning work to a machine you do **not** control. Back comes the "
    "output and a signed receipt. The question is: how do you know the node "
    "computed what it claims?"
)

prompt = st.text_area("Task for the network", "Write one sentence about compute verification.",
                      height=90)

if st.button("Submit job", type="primary", disabled=not node):
    stop = limity.sprawdz(LICZNIK, LIMIT_NA_GODZINE, jezyk="en") if PUBLICZNY else None
    if stop:
        st.warning(stop)
    else:
        with st.spinner("submitting job to the node..."):
            st.session_state.wynik = wywolaj([
                "--role", "agent", "--bootstrap", node, "--prompt", prompt[:2000],
                "--model-hash", model, "--max-tokens", str(max_tokens), "--json"]
                + (["--rejestr", REJESTR] if REJESTR else []))
        st.session_state.pop("werdykt", None)

if not node:
    st.info("Provide a node address in the panel on the left. The demo has no fake "
            "mode — without a working node there is nothing to verify.")

w = st.session_state.get("wynik")
if w and not w.get("ok"):
    st.error(f"Node did not execute the job: {w.get('powod') or w.get('opis') or w.get('kod')}")
elif w:
    lewo, prawo = st.columns([3, 2])
    with lewo:
        st.subheader("Output")
        st.write(w["output"])
        obs = w.get("obserwacja_klienta") or {}
        a, b, c = st.columns(3)
        # Jedyny czas, ktory ZMIERZYLISMY sami. Reszta to deklaracja wezla.
        a.metric("Time (measured at the client)", f"{obs.get('observed_time_to_complete_ms', '?')} ms")
        b.metric("Backend time (declared)", f"{w['ttft_ms']} ms")
        c.metric("Tokens (declared)", w["tokens_out"])
        st.caption(
            "TTFT is not measured — without streaming, the time to receive the "
            "whole response is not the same as the time to the first token, so we "
            "do not call it TTFT. The node's times are `node_declared_*`: the "
            "signature guarantees that the node declared them that way, not that it "
            "measured them correctly — **they must not be used for billing, "
            "slashing, or ranking**."
        )
        st.caption(
            f"signed work: prefill {w['receipt']['prompt_tokens']} tok. + "
            f"decode {w['receipt']['completion_tokens']} tok. — split out, because "
            "decoding costs ~55× more per token than prefill (measured)"
        )
    with prawo:
        st.subheader("Receipt")
        st.caption(f"signed by node `{w['receipt']['node_id'][:24]}…`")
        # Silnik i model bierzemy z PODPISANEGO receiptu, nie z naszego opisu
        # w panelu obok. To różnica między „twierdzimy, że to inny sprzęt"
        # a „węzeł sam to podpisał, więc możesz to sprawdzić".
        st.caption(f"computed by: `{w['receipt']['runtime']}` · model `{w['receipt']['model_hash']}`")
        st.json(w["receipt"], expanded=False)

    st.divider()
    st.subheader("Don't take it on faith — verify")
    st.markdown(
        "The receipt is signed with the node's Ed25519 key, and the signature covers "
        "**the digest of the response content** bound to the job identifier. Below "
        "you can verify it, and also try to fool the verification **in the three "
        "ways it would realistically be done**."
    )
    k1, k2, k3, k4 = st.columns(4)
    surowy = json.dumps(w["receipt"])

    if k1.button("Verify receipt"):
        st.session_state.werdykt = ("genuine receipt and genuine content", wywolaj(
            ["--verify-receipt", "-", "--expect-job-id", w["job_id"],
             "--expect-model", model, "--expect-output", do_pliku(w["output"]),
             "--json"], wejscie=surowy, limit_s=30))

    if k2.button("Swap the output"):
        podrobiony = dict(w["receipt"], output_digest="0" * 64)
        st.session_state.werdykt = ("node swaps the output after signing", wywolaj(
            ["--verify-receipt", "-", "--json"], wejscie=json.dumps(podrobiony), limit_s=30))

    if k4.button("Swap the response text"):
        # Najgrozniejszy z trzech: podpis jest PRAWDZIWY, wezel istnieje,
        # zlecenie sie zgadza — tylko tekst jest cudzy.
        st.session_state.werdykt = ("genuine signature, but SUBSTITUTED text", wywolaj(
            ["--verify-receipt", "-", "--json",
             "--expect-output", do_pliku("A completely different response that the node never computed.")],
            wejscie=surowy, limit_s=30))

    if k3.button("Replay under a different job"):
        st.session_state.werdykt = ("genuine receipt, but from a DIFFERENT job", wywolaj(
            ["--verify-receipt", "-", "--expect-job-id", "job-zupelnie-inne", "--json"],
            wejscie=surowy, limit_s=30))

    if "werdykt" in st.session_state:
        opis, v = st.session_state.werdykt
        st.caption(f"case: {opis}")
        if v.get("ok"):
            st.success("RECEIPT VALID")
            st.markdown(
                "- Ed25519 signature: **valid**\n"
                "- Content bound to the receipt: **yes**\n"
                "- Counters: **signed by the node** (not independently recomputed)\n"
                "- Model execution: **not yet audited**"
            )
        else:
            powody = []
            if v.get("parsuje_sie") is False:
                powody.append("this is not a valid receipt")
            if v.get("podpis_ok") is False:
                powody.append("**Ed25519 signature does not match** — content changed after signing")
            if v.get("job_id_ok") is False:
                powody.append("**receipt belongs to a different job** — the signature is "
                              "genuine, but this is not proof of THIS work")
            if v.get("model_ok") is False:
                powody.append("**a different model than the one ordered**")
            if v.get("tresc_ok") is False:
                powody.append("**this is not that output** — the signature is genuine, "
                              "but it describes different text")
            st.error("REJECTED: " + "; ".join(powody or [v.get("powod", "unknown reason")]))
        st.json(v, expanded=False)

    st.warning(
        "**What this does NOT prove.** The receipt proves that a registered node "
        "signed exactly this response as the output of this job. It does NOT prove "
        "that the declared model generated it — a malicious node can sign any text "
        "along with a correctly computed digest. Only an execution audit (TopLoc) "
        "binds the model, the prompt, the run, and the output. The token counters "
        "are a **declaration by the node**: the signature prevents them from being "
        "changed later, but does not make them true. The `ttft`/`gen` times also "
        "come from the node — only the round-trip time is measured independently."
    )

if REJESTR and pathlib.Path(REJESTR).is_file():
    st.divider()
    st.subheader("What this network has done so far")
    try:
        rekordy = [json.loads(l) for l in pathlib.Path(REJESTR).read_text().splitlines() if l.strip()]
    except (OSError, ValueError):
        rekordy = []
    if rekordy:
        per_wezel: dict[str, dict] = {}
        for r in rekordy:
            k = r["runtime_declared"]
            w = per_wezel.setdefault(k, {"zlecen": 0, "prefill": 0, "decode": 0, "ms": 0})
            w["zlecen"] += 1
            w["prefill"] += r["prompt_tokens_total"]
            w["decode"] += r["completion_tokens"]
            w["ms"] += r["client_observed_total_ms"]
        st.table([
            {"engine": k, "jobs": w["zlecen"], "prefill (tok.)": w["prefill"],
             "decode (tok.)": w["decode"], "client time (s)": round(w["ms"] / 1000, 1)}
            for k, w in per_wezel.items()
        ])
        st.caption(
            f"**{len(rekordy)} units of work.** The output, the counters, and the "
            "authorship are bound by a signed receipt; the verification level is shown "
            "separately and today it is *signature + content binding* — **not** an "
            "execution audit. This is a meter, not a wallet: there is no balance, no "
            "reward, and no conversion rate here, because \"done and verified\" does "
            "not mean \"bought by independent demand\". The token counters come from "
            "the node's declaration; the only independently measured quantity is the "
            "time on the client side."
        )

st.divider()
st.caption(
    "A full audit (re-running the job through a verifier) costs 2.4–9.9% of the "
    "cost of the job itself — measured, `docs/design/DESIGN-VERIFIER-0.md`. "
    "What SIMON still does NOT do: it does not automatically penalize a mismatched "
    "activation digest, because we do not have a threshold we could defend."
)
