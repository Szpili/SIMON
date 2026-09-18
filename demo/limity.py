"""Globalny limit zleceń dla publicznego demo.

Za każdym zleceniem stoi prawdziwe GPU. Bez tego wystawione demo jest darmową
inferencją dla całego internetu — i pierwszy bot, który je znajdzie, zajmie
kartę na tyle długo, żeby demo nie działało dokładnie wtedy, gdy ktoś je ocenia.

ponytail: licznik w pliku JSON, bez bazy. Wystarcza dla jednego procesu demo;
przy kilku replikach trzeba czegoś współdzielonego (Redis albo limit w proxy).
"""
import json
import time
from pathlib import Path


def sprawdz(plik: Path, limit_na_godzine: int, teraz: float | None = None) -> str | None:
    """Zwraca None, gdy wolno; komunikat dla użytkownika, gdy limit wyczerpany.

    Wywołanie, które przechodzi, OD RAZU zapisuje swój znacznik — inaczej dwa
    kliknięcia w tej samej sekundzie obeszłyby limit.
    """
    teraz = time.time() if teraz is None else teraz
    try:
        znaczniki = [float(t) for t in json.loads(plik.read_text())
                     if teraz - float(t) < 3600]
    except (OSError, ValueError, TypeError):
        # Uszkodzony albo brakujący licznik traktujemy jak pusty, ale NIE
        # odpuszczamy limitu — zapis poniżej odtwarza plik.
        znaczniki = []

    if len(znaczniki) >= limit_na_godzine:
        za_ile = int((3600 - (teraz - min(znaczniki))) / 60) + 1
        return (f"limit demo: {limit_na_godzine} zleceń na godzinę. "
                f"Spróbuj za {za_ile} min.")

    znaczniki.append(teraz)
    try:
        plik.write_text(json.dumps(znaczniki))
    except OSError:
        pass  # brak zapisu nie może zablokować demo — odsłania je, nie psuje
    return None


def _samotest() -> None:
    import tempfile
    p = Path(tempfile.mkdtemp()) / "licznik.json"
    t = 1_000_000.0

    assert sprawdz(p, 3, t) is None
    assert sprawdz(p, 3, t) is None
    assert sprawdz(p, 3, t) is None
    assert sprawdz(p, 3, t) is not None, "czwarte zlecenie musi zostać odrzucone"

    # po godzinie okno się przesuwa
    assert sprawdz(p, 3, t + 3601) is None, "stare znaczniki muszą wygasać"

    # uszkodzony licznik nie może wysypać demo ani otworzyć go bez limitu
    p.write_text("{to nie jest JSON")
    assert sprawdz(p, 1, t) is None
    assert sprawdz(p, 1, t) is not None, "po odtworzeniu licznik ma dalej działać"

    print("limity: OK")


if __name__ == "__main__":
    _samotest()
