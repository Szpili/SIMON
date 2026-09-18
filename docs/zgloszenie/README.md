# Materiały zgłoszeniowe — AMD Developer Hackathon: ACT III

Wymagania platformy (lablab.ai): okładka PNG/JPG **16:9**, prezentacja **PDF**,
wideo **MP4**, publiczne repo GitHub, URL działającej aplikacji.

| Element | Plik / adres | Stan |
|---|---|---|
| Okładka 16:9 | `okladka-1920x1080.png` (źródło: `okladka.svg`) | gotowe |
| Slajdy | deck w Artifacts, eksport do PDF z widoku prezentacji | gotowe (10 slajdów) |
| Repo | `github.com/Szpili/SIMON` | gotowe, MIT |
| Demo pod URL-em | Tailscale Funnel, usługi pod systemd | działa |
| Wideo MP4 | — | **brak** — ma sens dopiero z wynikiem ROCm |

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
