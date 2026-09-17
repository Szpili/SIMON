#!/usr/bin/env python3
"""Skład whitepapera SIMON do PDF (pandoc -> HTML -> weasyprint).

Użycie:  python3 build.py <plik.md> <wyjscie.pdf> <pl|en>
"""
import re
import subprocess
import sys
from pathlib import Path

TU = Path(__file__).resolve().parent

TEKSTY = {
    "pl": {
        "doctype": "Whitepaper",
        "tagline": "Rozproszony habitat dla otwartych wag modeli. "
                   "Ludzie z GPU liczą inferencję, sieć weryfikuje, że policzyli naprawdę.",
        "principle": "Zasada nadrzędna: <b>bez dowodu nie ma statusu</b> "
                     "(ZAPROJEKTOWANE / ZAIMPLEMENTOWANE / ZMIERZONE).",
        "footer": "SIMON — Whitepaper",
    },
    "en": {
        "doctype": "Whitepaper",
        "tagline": "A distributed habitat for open weights. People with GPUs compute "
                   "inference, and the network verifies they actually did the work.",
        "principle": "Governing rule: <b>no evidence, no status</b> "
                     "(DESIGNED / IMPLEMENTED / MEASURED).",
        "footer": "SIMON — Whitepaper",
    },
}

# slowa-statusy -> plakietki (PL i EN)
BADGES = [
    (r"\bZMIERZONE\b", "st-m"), (r"\bMEASURED\b", "st-m"),
    (r"\bZAIMPLEMENTOWANE\b", "st-i"), (r"\bIMPLEMENTED\b", "st-i"),
    (r"\bZAPROJEKTOWANE\b", "st-d"), (r"\bDESIGNED\b", "st-d"),
]


def rozdziel_naglowek(md: str):
    """Oddziela blok metadanych na górze (idzie na okładkę) od treści."""
    linie = md.splitlines()
    tytul = linie[0].lstrip("# ").strip() if linie else "SIMON"
    meta, i = [], 1
    while i < len(linie) and linie[i].strip() != "---":
        if linie[i].strip():
            meta.append(linie[i].strip())
        i += 1
    tresc = "\n".join(linie[i + 1:]) if i < len(linie) else "\n".join(linie[1:])
    return tytul, meta, tresc


def md_na_html(md: str) -> str:
    # pandoc na tym hoście to zaślepka — używamy Python-Markdown (3.10).
    import markdown
    return markdown.markdown(
        md,
        extensions=["tables", "fenced_code", "sane_lists", "attr_list"],
        output_format="html5",
    )


def znaczniki(html: str) -> str:
    """Emoji nie maja pokrycia w fontach do druku (renderuja sie jako drobinki).
    Zamiana na typograficzne znaczniki z klasami CSS."""
    zamiany = [
        ("❌", '<span class="mk mk-no">✗</span>'),        # X -> krzyzyk
        ("✅", '<span class="mk mk-ok">✓</span>'),        # V -> ptaszek
        ("⚠️", '<span class="mk mk-warn">!</span>'),     # ostrzezenie (z VS16)
        ("⚠", '<span class="mk mk-warn">!</span>'),           # ostrzezenie (bez VS16)
    ]
    for zrodlo, cel in zamiany:
        html = html.replace(zrodlo, cel)
    return html


def plakietki(html: str) -> str:
    """Otacza slowa-statusy plakietka, ale NIE w naglowkach ani w <code>."""
    czesci = re.split(r"(<code>.*?</code>|<h[1-6][^>]*>.*?</h[1-6]>)", html, flags=re.S)
    for idx, cz in enumerate(czesci):
        if cz.startswith("<code>") or re.match(r"<h[1-6]", cz):
            continue
        for wzor, klasa in BADGES:
            cz = re.sub(wzor, lambda m, k=klasa: f'<span class="st {k}">{m.group(0)}</span>', cz)
        czesci[idx] = cz
    return "".join(czesci)


def okladka(tytul: str, meta: list, jezyk: str) -> str:
    t = TEKSTY[jezyk]
    mark = (TU / "assets" / "simon-mark.svg").read_text()
    # metadane: "**Klucz:** wartosc" -> wiersz
    wiersze = []
    for m in meta:
        m = re.sub(r"\*\*(.+?):\*\*", r"<b>\1:</b>", m)
        m = re.sub(r"\*\*(.+?)\*\*", r"<b>\1</b>", m)
        m = re.sub(r"`(.+?)`", r"<code>\1</code>", m)
        if m.lower().startswith("<b>zasada") or m.lower().startswith("<b>governing"):
            continue  # zasada ma wlasny box
        wiersze.append(m)
    return f"""<section class="cover">
  <div class="mark">{mark}</div>
  <div class="wordmark">SIMON</div>
  <div class="rule"></div>
  <div class="doctype">{t['doctype']}</div>
  <p class="tagline">{t['tagline']}</p>
  <div class="spacer"></div>
  <div class="meta">
    {"<br>".join(wiersze)}
    <div class="principle">{t['principle']}</div>
  </div>
</section>
<h1 class="doctitle" style="display:none">{t['footer']}</h1>"""


def main():
    zrodlo, wyjscie, jezyk = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    md = zrodlo.read_text(encoding="utf-8")
    tytul, meta, tresc = rozdziel_naglowek(md)

    body = plakietki(znaczniki(md_na_html(tresc)))
    css = (TU / "wp.css").read_text()

    html = f"""<!DOCTYPE html>
<html lang="{jezyk}"><head><meta charset="utf-8">
<title>{tytul}</title><style>{css}</style></head>
<body>
{okladka(tytul, meta, jezyk)}
<main>{body}</main>
</body></html>"""

    tmp_html = TU / f".build-{jezyk}.html"
    tmp_html.write_text(html, encoding="utf-8")
    subprocess.run(["weasyprint", "-u", str(TU) + "/", str(tmp_html), str(wyjscie)], check=True)
    print(f"OK -> {wyjscie}  ({wyjscie.stat().st_size/1024:.0f} kB)")


if __name__ == "__main__":
    main()
