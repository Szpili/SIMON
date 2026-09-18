# Materiały zgłoszeniowe — AMD Developer Hackathon: ACT III

Wymagania platformy (lablab.ai): okładka PNG/JPG **16:9**, prezentacja **PDF**,
wideo **MP4**, publiczne repo GitHub, URL działającej aplikacji.

| Element | Plik / adres | Stan |
|---|---|---|
| Okładka 16:9 | `okladka-1920x1080.png` (źródło: `okladka.svg`) | gotowe |
| Slajdy | `SIMON-slajdy.pdf` (10 stron, 1440×810 pt = 16:9); źródła w `slajdy/` | gotowe |
| Repo | `github.com/Szpili/SIMON` | gotowe, MIT |
| Demo pod URL-em | Tailscale Funnel, usługi pod systemd | działa |
| Wideo MP4 | — | **brak** — ma sens dopiero z wynikiem ROCm |

## Slajdy

Źródłem prawdy są pliki w `slajdy/` — jeden slajd to jedna sekcja na płótnie
1920×1080. Ten sam zestaw zasila deck w Artifacts i PDF do zgłoszenia, więc nie
mogą się rozjechać.

```bash
python3 buduj_slajdy.py            # HTML + PDF
python3 buduj_slajdy.py --arkusz   # dodatkowo arkusz miniatur do przeglądu
```

Arkusz miniatur nie jest ozdobą: **osiem z dziesięciu slajdów było w pierwszej
wersji przepełnionych** — stopki nachodziły na tekst, a jedna karta w ogóle nie
mieściła się na slajdzie. Widać to dopiero po wyrenderowaniu wszystkich naraz,
nie po przeczytaniu HTML-a.

Notatki prelegenta (`<aside>`) **nie trafiają na slajd ani do PDF-a** — są
widoczne tylko w widoku prezentacji.

### Pułapka, która kosztowała dwie rundy

Złota kreska pod tytułem to `border-top`, a **nie** pudełko o wysokości 8 px.
W kolumnie flex taki element jest ściskany do zera i znika bez śladu —
obramowania kurczeniu nie podlegają.

## Okładka

Generowana, nie rysowana ręcznie: `okladka.svg` powstaje ze skryptu, więc
gęstość plastra miodu i układ da się dostroić bez grzebania w XML-u. Render:

```bash
python3 -c "import cairosvg; cairosvg.svg2png(url='okladka.svg', write_to='okladka-1920x1080.png', output_width=1920, output_height=1080)"
```

Paleta i typografia są **te same co w whitepaperze** (`docs/whitepaper/wp.css`):
granat `#151B2D`, złoto `#D9A441`, papier `#F5F4F1`, Fira Sans. Druga tożsamość
wizualna dla tego samego projektu byłaby błędem, nie urozmaiceniem.

Projektowana pod **miniaturę w galerii setek zgłoszeń**, nie pod pełny ekran:
wielki wordmark, jedna linia twierdzenia, jeden akcent.
