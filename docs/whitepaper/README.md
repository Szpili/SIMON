# Whitepaper SIMON — skład do PDF

Dwie wersje językowe z jednego warsztatu: `SIMON-whitepaper-PL.md` i
`SIMON-whitepaper-EN.md` → PDF przez `build.py`.

```bash
python3 build.py SIMON-whitepaper-PL.md SIMON-Whitepaper-PL.pdf pl
python3 build.py SIMON-whitepaper-EN.md SIMON-Whitepaper-EN.pdf en
```

**Zależności:** `weasyprint` + `python3-markdown` + fonty Fira Sans/Fira Mono.
(`pandoc` na Szponie to zaślepka — dlatego markdown idzie przez Python-Markdown.)

**Uwagi do składu:**
- Logotyp `assets/simon-mark.svg` odtworzony jako wektor (7 hexów flat-top,
  środkowy bursztynowy). SVG celowo BEZ atrybutów `width`/`height` — rozmiar
  ustala CSS, inaczej nadpisuje okładkę.
- Emoji (`❌ ⚠️ ✅`) nie mają pokrycia w fontach do druku i renderują się jako
  nieczytelne drobinki — `build.py` zamienia je na znaczniki z klasami CSS.
- Słowa-statusy (ZMIERZONE/MEASURED itd.) dostają plakietki, ale NIE w
  nagłówkach ani w `<code>`.

**Wersja EN NIE jest tłumaczeniem starego short papera** (`docs/WHITEPAPER-SHORT-*`),
bo tamten zawierał twierdzenia nieaktualne od 2026-09-17 („network not yet
running", „no binary", „never run on two machines"). To pełne tłumaczenie
aktualnego whitepapera.
