#!/usr/bin/env python3
"""Assembles slides into a single HTML file and renders PDF.

Slides are the source of truth - each is one section on a 1920x1080 canvas.

Usage:
    python3 buduj_slajdy.py            # HTML + PDF
    python3 buduj_slajdy.py --arkusz   # slide contact sheet for review
"""
import json
import pathlib
import shutil
import subprocess
import sys

KAT = pathlib.Path(__file__).resolve().parent
SLAJDY = KAT / "slajdy"
CZCIONKI = ("https://fonts.googleapis.com/css2?family=Fira+Sans:wght@400;500;700"
            "&family=Fira+Mono:wght@400;500&display=swap")


def zloz_html(do_druku: bool) -> str:
    deck = json.loads((SLAJDY / "deck.json").read_text())
    sekcje = "".join((SLAJDY / f"{s}.html").read_text() for s in deck["order"])
    # @page in px matching slide canvas - avoids Chrome scaling issues.
    druk = """
      @page { size: 1920px 1080px; margin: 0 }
      section { page-break-after: always; break-after: page }
    """ if do_druku else ""
    tlo = "" if do_druku else "body{background:#2a2a2a}"
    return f"""<!doctype html><html lang="en"><head><meta charset="utf-8">
<title>{deck['title']}</title>
<link rel="stylesheet" href="{CZCIONKI}">
<style>
  body {{ margin:0 }} {tlo}
  section {{ width:1920px; height:1080px; box-sizing:border-box;
             overflow:hidden; position:relative }}
  aside {{ display:none }}   /* speaker notes omitted from slide */
  {druk}
</style></head><body>{sekcje}</body></html>"""


def przegladarka() -> str:
    for b in ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser"):
        if shutil.which(b):
            return b
    raise SystemExit("missing Chrome/Chromium - cannot render PDF")


def main() -> None:
    (KAT / "slajdy.html").write_text(zloz_html(do_druku=False))
    druk = KAT / ".slajdy-druk.html"
    druk.write_text(zloz_html(do_druku=True))
    pdf = KAT / "SIMON-slajdy.pdf"
    subprocess.run([przegladarka(), "--headless", "--disable-gpu", "--no-sandbox",
                    "--no-pdf-header-footer", f"--print-to-pdf={pdf}", str(druk)],
                   capture_output=True, timeout=300, check=False)
    druk.unlink(missing_ok=True)
    if not pdf.exists():
        raise SystemExit("PDF failed to generate")
    print(f"OK -> {pdf.name} ({pdf.stat().st_size // 1024} kB)")

    if "--arkusz" in sys.argv:
        from PIL import Image, ImageDraw
        pelny = KAT / ".arkusz.png"
        deck = json.loads((SLAJDY / "deck.json").read_text())
        n = len(deck["order"])
        subprocess.run([przegladarka(), "--headless", "--disable-gpu", "--no-sandbox",
                        "--hide-scrollbars", f"--window-size=1920,{n * 1080}",
                        f"--screenshot={pelny}", str(KAT / 'slajdy.html')],
                       capture_output=True, timeout=300, check=False)
        im = Image.open(pelny)
        mini = (480, 270)
        ark = Image.new("RGB", (2 * mini[0], ((n + 1) // 2) * mini[1]), "#333")
        d = ImageDraw.Draw(ark)
        for i, sid in enumerate(deck["order"]):
            ark.paste(im.crop((0, i * 1080, 1920, (i + 1) * 1080)).resize(mini),
                      ((i % 2) * mini[0], (i // 2) * mini[1]))
            d.text(((i % 2) * mini[0] + 6, (i // 2) * mini[1] + 4), f"{i}:{sid}", fill="#0f0")
        ark.save(KAT / "arkusz-slajdow.png")
        pelny.unlink(missing_ok=True)
        print("OK -> arkusz-slajdow.png")


if __name__ == "__main__":
    main()
