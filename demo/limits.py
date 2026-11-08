"""Global order rate limiter for the public demo.

Rate limiting file counter for single-instance demo.
"""
import json
import time
from pathlib import Path


def sprawdz(plik: Path, limit_na_godzine: int, teraz: float | None = None,
            jezyk: str = "en") -> str | None:
    """Returns None if permitted, or user error message if limit is exhausted."""
    teraz = time.time() if teraz is None else teraz
    try:
        znaczniki = [float(t) for t in json.loads(plik.read_text())
                     if teraz - float(t) < 3600]
    except (OSError, ValueError, TypeError):
        znaczniki = []

    if len(znaczniki) >= limit_na_godzine:
        za_ile = int((3600 - (teraz - min(znaczniki))) / 60) + 1
        if jezyk == "en":
            return (f"demo limit: {limit_na_godzine} jobs per hour. "
                    f"Try again in {za_ile} min.")
        return (f"limit demo: {limit_na_godzine} zlece\u0144 na godzin\u0119. "
                f"Spr\u00f3buj za {za_ile} min.")

    znaczniki.append(teraz)
    try:
        plik.write_text(json.dumps(znaczniki))
    except OSError:
        pass
    return None


def _samotest() -> None:
    import tempfile
    p = Path(tempfile.mkdtemp()) / "licznik.json"
    t = 1_000_000.0

    assert sprawdz(p, 3, t) is None
    assert sprawdz(p, 3, t) is None
    assert sprawdz(p, 3, t) is None
    assert sprawdz(p, 3, t) is not None, "fourth job must be rejected"

    # window slides after 1 hour
    assert sprawdz(p, 3, t + 3601) is None, "old stamps must expire"

    # corrupted counter must not crash demo
    p.write_text("{invalid json")
    assert sprawdz(p, 1, t) is None
    assert sprawdz(p, 1, t) is not None, "counter must continue after recreation"

    print("limits: OK")


if __name__ == "__main__":
    _samotest()
