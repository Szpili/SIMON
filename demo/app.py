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
            st.warning("nie mogę odczytać listy węzłów — używam domyślnego")
    if NODE:
        return [{"nazwa": "węzeł domyślny", "adres": NODE,
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
        return {"ok": False, "powod": f"nie znaleziono binarki: {SIMON}"}
    except subprocess.TimeoutExpired:
        return {"ok": False, "powod": f"węzeł nie odpowiedział w {limit_s}s"}
    for linia in reversed((p.stdout or "").strip().splitlines()):
        try:
            return json.loads(linia)
        except json.JSONDecodeError:
            continue
    return {"ok": False, "powod": (p.stderr or "brak odpowiedzi").strip()[-400:]}


with st.sidebar:
    st.header("Węzeł")
    if PUBLICZNY:
        # Wybór TYLKO z listy — adres nigdy nie pochodzi od odwiedzającego.
        if WEZLY:
            wybrany = st.radio("Komu zlecić", WEZLY,
                               format_func=lambda w: w["nazwa"])
            node, model = wybrany["adres"], wybrany["model"]
            if wybrany.get("opis"):
                st.caption(wybrany["opis"])
        else:
            node, model = "", ""
        max_tokens = st.slider("Limit tokenów", 20, 200, 80, step=20)
        st.caption(f"demo publiczne — limit {LIMIT_NA_GODZINE} zleceń/godz.")
    else:
        node = st.text_input("Adres węzła (multiaddr)", NODE,
                             placeholder="/ip4/1.2.3.4/tcp/9001/p2p/12D3Koo...")
        model = st.text_input("Model", "qwen3.8-27b")
        max_tokens = st.slider("Limit tokenów", 20, 400, 80, step=20)
        st.caption(f"binarka: `{SIMON}`")

st.title("SIMON")
st.markdown(
    "Zlecasz pracę maszynie, której **nie kontrolujesz**. Wraca wynik i podpisany "
    "receipt. Pytanie brzmi: skąd wiesz, że węzeł policzył to, co twierdzi?"
)

prompt = st.text_area("Zadanie dla sieci", "Napisz jedno zdanie o weryfikacji obliczeń.",
                      height=90)

if st.button("Zleć zadanie", type="primary", disabled=not node):
    stop = limity.sprawdz(LICZNIK, LIMIT_NA_GODZINE) if PUBLICZNY else None
    if stop:
        st.warning(stop)
    else:
        with st.spinner("zlecenie leci do węzła..."):
            st.session_state.wynik = wywolaj([
                "--role", "agent", "--bootstrap", node, "--prompt", prompt[:2000],
                "--model-hash", model, "--max-tokens", str(max_tokens), "--json"])
        st.session_state.pop("werdykt", None)

if not node:
    st.info("Podaj adres węzła w panelu po lewej. Demo nie ma trybu udawanego — "
            "bez działającego węzła nie ma czego weryfikować.")

w = st.session_state.get("wynik")
if w and not w.get("ok"):
    st.error(f"Węzeł nie wykonał zlecenia: {w.get('powod') or w.get('opis') or w.get('kod')}")
elif w:
    lewo, prawo = st.columns([3, 2])
    with lewo:
        st.subheader("Wynik")
        st.write(w["output"])
        a, b, c = st.columns(3)
        a.metric("TTFT", f"{w['ttft_ms']} ms")
        b.metric("Przepustowość", f"{w['tok_s']} tok/s")
        c.metric("Tokenów", w["tokens_out"])
        st.caption(
            f"podpisana praca: prefill {w['receipt']['prompt_tokens']} tok. + "
            f"decode {w['receipt']['completion_tokens']} tok. — rozdzielone, bo "
            "dekodowanie kosztuje ~55× więcej na token niż prefill (zmierzone)"
        )
    with prawo:
        st.subheader("Receipt")
        st.caption(f"podpisał węzeł `{w['receipt']['node_id'][:24]}…`")
        # Silnik i model bierzemy z PODPISANEGO receiptu, nie z naszego opisu
        # w panelu obok. To różnica między „twierdzimy, że to inny sprzęt"
        # a „węzeł sam to podpisał, więc możesz to sprawdzić".
        st.caption(f"policzył: `{w['receipt']['runtime']}` · model `{w['receipt']['model_hash']}`")
        st.json(w["receipt"], expanded=False)

    st.divider()
    st.subheader("Nie wierz na słowo — sprawdź")
    st.markdown(
        "Receipt jest podpisany kluczem Ed25519 węzła, a podpis obejmuje **odcisk "
        "treści odpowiedzi** związany z identyfikatorem zlecenia. Poniżej możesz go "
        "zweryfikować, a także spróbować oszukać weryfikację **trzema sposobami, "
        "którymi realnie by się to zrobiło**."
    )
    k1, k2, k3, k4 = st.columns(4)
    surowy = json.dumps(w["receipt"])

    if k1.button("Zweryfikuj receipt"):
        st.session_state.werdykt = ("prawdziwy receipt i prawdziwa treść", wywolaj(
            ["--verify-receipt", "-", "--expect-job-id", w["job_id"],
             "--expect-model", model, "--expect-output", do_pliku(w["output"]),
             "--json"], wejscie=surowy, limit_s=30))

    if k2.button("Podmień wynik"):
        podrobiony = dict(w["receipt"], output_digest="0" * 64)
        st.session_state.werdykt = ("węzeł podmienia wynik po podpisaniu", wywolaj(
            ["--verify-receipt", "-", "--json"], wejscie=json.dumps(podrobiony), limit_s=30))

    if k4.button("Podmień treść odpowiedzi"):
        # Najgrozniejszy z trzech: podpis jest PRAWDZIWY, wezel istnieje,
        # zlecenie sie zgadza — tylko tekst jest cudzy.
        st.session_state.werdykt = ("prawdziwy podpis, ale PODSTAWIONY tekst", wywolaj(
            ["--verify-receipt", "-", "--json",
             "--expect-output", do_pliku("Zupełnie inna odpowiedź, której węzeł nigdy nie policzył.")],
            wejscie=surowy, limit_s=30))

    if k3.button("Podstaw pod inne zlecenie"):
        st.session_state.werdykt = ("prawdziwy receipt, ale z CUDZEGO zlecenia", wywolaj(
            ["--verify-receipt", "-", "--expect-job-id", "job-zupelnie-inne", "--json"],
            wejscie=surowy, limit_s=30))

    if "werdykt" in st.session_state:
        opis, v = st.session_state.werdykt
        st.caption(f"przypadek: {opis}")
        if v.get("ok"):
            st.success("RECEIPT WAŻNY")
            st.markdown(
                "- Podpis Ed25519: **poprawny**\n"
                "- Treść związana z receiptem: **tak**\n"
                "- Liczniki: **podpisane przez węzeł** (nie przeliczone niezależnie)\n"
                "- Wykonanie modelu: **jeszcze niezaudytowane**"
            )
        else:
            powody = []
            if v.get("parsuje_sie") is False:
                powody.append("to nie jest poprawny receipt")
            if v.get("podpis_ok") is False:
                powody.append("**podpis Ed25519 nie pasuje** — treść zmieniona po podpisaniu")
            if v.get("job_id_ok") is False:
                powody.append("**receipt dotyczy innego zlecenia** — podpis prawdziwy, "
                              "ale to nie jest dowód na TĘ pracę")
            if v.get("model_ok") is False:
                powody.append("**inny model niż zamówiony**")
            if v.get("tresc_ok") is False:
                powody.append("**to nie jest ten wynik** — podpis prawdziwy, ale opisuje inny tekst")
            st.error("ODRZUCONY: " + "; ".join(powody or [v.get("powod", "nieznany powód")]))
        st.json(v, expanded=False)

    st.warning(
        "**Czego to NIE dowodzi.** Receipt dowodzi, że zarejestrowany węzeł "
        "podpisał dokładnie tę odpowiedź jako wynik tego zlecenia. NIE dowodzi, "
        "że wygenerował ją zadeklarowany model — złośliwy węzeł może podpisać "
        "dowolny tekst wraz z poprawnie policzonym odciskiem. Dopiero audyt "
        "wykonania (TopLoc) wiąże model, prompt, przebieg i wynik. "
        "Liczniki tokenów są **deklaracją węzła**: podpis uniemożliwia ich "
        "późniejszą zmianę, ale nie czyni ich prawdziwymi. Czasy `ttft`/`gen` "
        "też pochodzą od węzła — mierzony niezależnie jest tylko czas obiegu."
    )

st.divider()
st.caption(
    "Audyt pełny (przeliczenie zlecenia przez weryfikatora) kosztuje 2,4–9,9% "
    "kosztu samego zlecenia — zmierzone, `docs/design/DESIGN-VERIFIER-0.md`. "
    "Czego SIMON jeszcze NIE robi: nie karze automatycznie za rozbieżny odcisk "
    "aktywacji, bo nie mamy progu, którego dalibyśmy radę obronić."
)
