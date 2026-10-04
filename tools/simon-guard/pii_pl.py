#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Deterministyczna warstwa regul anonimizacji PII strukturalnej (PL). Bez zaleznosci zewnetrznych.

Kontrakt:
    a = Anonimizer()
    red, stat = a.redact(text)      # -> placeholdery [TYP#n]
    back = a.restore(red)           # -> oryginaly (tylko ta sama instancja)

Zasady:
- Typ strukturalny wchodzi TYLKO z suma kontrolna (PESEL/NIP/REGON/IBAN) albo z jawnym kontekstem
  (telefon z +48, e-mail, sygnatura akt, numer KW, kod pocztowy).
- 11 cyfr BEZ poprawnej sumy, ale z prawdopodobna data urodzenia => typ podejrzany [PESEL?]
  (swiadome odejscie w strone czulosci: PESEL z literowka nie moze wyciec).
- Mapa placeholder->oryginal zyje wylacznie w pamieci procesu. NIE jest logowana i nie wychodzi na zewnatrz.
- Brak wykrywania nazwisk/adresow - to warstwa modelu (NER), nie reguly.
"""
from __future__ import annotations

import re
import unicodedata
from typing import Dict, List, Tuple

# ---------------------------------------------------------------- sumy kontrolne

def _cyfry(s: str) -> str:
    return re.sub(r"\D", "", s)


def pesel_ok(s: str) -> bool:
    d = _cyfry(s)
    if len(d) != 11:
        return False
    w = (1, 3, 7, 9, 1, 3, 7, 9, 1, 3)
    c = (10 - sum(int(x) * y for x, y in zip(d[:10], w)) % 10) % 10
    return c == int(d[10])


def pesel_data_plausible(s: str) -> bool:
    """Data urodzenia kodowana w PESEL: MM z przesunieciem epoki (18xx/19xx/20xx/21xx/22xx)."""
    d = _cyfry(s)
    if len(d) != 11:
        return False
    m = int(d[2:4])
    return m in list(range(1, 13)) + list(range(21, 33)) + list(range(41, 53)) + \
        list(range(61, 73)) + list(range(81, 93))


def nip_ok(s: str) -> bool:
    d = _cyfry(s)
    if len(d) != 10:
        return False
    w = (6, 5, 7, 2, 3, 4, 5, 6, 7)
    r = sum(int(x) * y for x, y in zip(d[:9], w)) % 11
    return r != 10 and r == int(d[9])


def regon9_ok(s: str) -> bool:
    d = _cyfry(s)
    if len(d) != 9:
        return False
    w = (8, 9, 2, 3, 4, 5, 6, 7)
    r = sum(int(x) * y for x, y in zip(d[:8], w)) % 11
    return (0 if r == 10 else r) == int(d[8])


def regon14_ok(s: str) -> bool:
    d = _cyfry(s)
    if len(d) != 14:
        return False
    w = (2, 4, 8, 5, 0, 9, 7, 3, 6, 1, 2, 4, 8)
    r = sum(int(x) * y for x, y in zip(d[:13], w)) % 11
    return (0 if r == 10 else r) == int(d[13])


def iban_pl_ok(s: str) -> bool:
    s = re.sub(r"\s", "", s).upper()
    if not re.fullmatch(r"PL\d{26}", s):
        return False
    prz = s[4:] + s[:4]
    num = "".join(str(ord(c) - 55) if c.isalpha() else c for c in prz)
    return int(num) % 97 == 1


# ---------------------------------------------------------------- wzorce

def _re(pat: str) -> re.Pattern:
    return re.compile(pat, re.UNICODE)


WZORCE: List[Tuple[str, re.Pattern]] = [
    # ---- SEKRETY PIERWSZE: bloki kluczy i dlugie ciagi musza zostac zredagowane ZANIM
    #      reguly PII (PESEL/NIP/REGON) potna cyfry wewnatrz klucza i rozbija dopasowanie.
    # klucz prywatny SSH/PGP - blok wielolinijkowy
    ("KLUCZ_SSH", _re(r"-----BEGIN [A-Z ]{0,20}PRIVATE KEY-----[\s\S]{0,4000}?-----END [A-Z ]{0,20}PRIVATE KEY-----")),
    # klucze z prefiksem producenta (OpenAI/Anthropic/NVIDIA/Google/GitHub/HF/Slack/Stripe/GitLab/AWS)
    ("KLUCZ_API", _re(r"\bsk-ant-[A-Za-z0-9_-]{16,}"
                      r"|\bsk-proj-[A-Za-z0-9_-]{16,}"
                      r"|\bsk_live_[A-Za-z0-9]{16,}"
                      r"|\bsk-[A-Za-z0-9_-]{20,}"
                      r"|\bnvapi-[A-Za-z0-9_-]{16,}"
                      r"|\bAIza[0-9A-Za-z_-]{30,}"
                      r"|\bhf_[A-Za-z0-9]{24,}"
                      r"|\bglpat-[A-Za-z0-9_-]{16,}"
                      r"|\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}"
                      r"|\bxox[baprs]-[A-Za-z0-9-]{10,}"
                      r"|\bAKIA[0-9A-Z]{16}\b")),
    # tokeny: JWT, Bearer, OAuth
    ("TOKEN", _re(r"\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}=*"
                  r"|(?i:\bbearer)\s+[A-Za-z0-9._~+/=-]{20,}"
                  r"|(?i:\bbasic)\s+[A-Za-z0-9+/=]{16,}")),
    # connection string z haslem w srodku (postgres://user:haslo@host)
    ("CONNSTR", _re(r"(?i)\b(?:postgres(?:ql)?|mysql|mariadb|mongodb(?:\+srv)?|redis|amqp|mssql|ftp)://"
                    r"[^\s\"'<>]{1,60}:[^\s\"'<>@]{1,60}@[^\s\"'<>]{3,120}")),
    # przypisanie sekretu: API_KEY=..., SECRET_KEY: ..., access_token=..., aws_access_key_id=...
    ("SEKRET", _re(r"(?i)\b(?:api[_-]?key|apikey|api[_-]?secret|access[_-]?key(?:_id)?|secret[_-]?key|"
                   r"client[_-]?secret|private[_-]?key|auth[_-]?token|access[_-]?token|refresh[_-]?token|"
                   r"aws_secret_access_key|secret|token|haslo|haslo_hash)\b\s*[:=]\s*[\"']?([^\s\"',;)]{8,})")),
    # haslo w zdaniu: "haslo do bazy: X", "haslo to X", "password is X", "PIN 4321"
    ("HASLO", _re(r"(?i)\b(?:has[lł]o|has[lł]a|password|passwd|pwd|passphrase)\b[^:=\n]{0,28}[:=]\s*[\"']?([^\s\"',;)]{4,})"
                  r"|\b(?:has[lł]o|password)\b\s+(?:to|jest|is)\s+[\"']?([^\s\"',;)]{4,})"
                  r"|\bpin\b[^.\n]{0,24}?\b\d{4}\b")),
    # kwota/IBAN najpierw - najdluzszy ciag strukturalny
    ("IBAN", _re(r"\bPL\s?\d{2}(?:\s?\d{4}){6}\b")),
    # 11 cyfr (PESEL) - walidacja pozniej, po dopasowaniu
    ("PESEL", _re(r"(?<!\d)\d{11}(?!\d)")),
    # NIP: 10 cyfr, dopuszczalne separatory w grupach 3-3-2-3 / 3-3-4
    ("NIP", _re(r"(?<![\d-])\d{3}[-\s]\d{3}[-\s]\d{2}[-\s]\d{2}(?![\d-])|(?<![\d-])\d{10}(?![\d-])")),
    # REGON 14 przed 9 (dluzszy wygrywa)
    ("REGON", _re(r"(?<!\d)\d{14}(?!\d)|(?<!\d)\d{9}(?!\d)")),
    # telefon PL: +48 z 9 cyframi, albo 9 cyfr w grupach 3-3-3, albo 3-2-2-2
    ("TEL", _re(r"(?:\+48|0048)[\s-]?\d{3}[\s-]?\d{3}[\s-]?\d{3}\b"
                r"|\b\d{3}[\s-]\d{3}[\s-]\d{3}\b"
                r"|\b\d{3}[\s-]\d{2}[\s-]\d{2}[\s-]\d{2}\b")),
    ("EMAIL", _re(r"\b[\w.!#$%&'*+/=?^`{|}~-]+@[\w-]+(?:\.[\w-]+)+\b")),
    # sygnatura akt: opcjonalny prefiks slowa, num rzymskie/arabs + kod + liczba/rok
    ("SYGN", _re(r"\b(?:[IVXLC]{1,6}|[A-Z]{1,3})\s+[A-Z]{1,3}\s+\d{1,6}\s?/\s?\d{2,4}\b"
                 r"|\bsygn\.?\s*akt\s+(?:[IVXLC]{1,6}|[A-Z]{1,3})\s+[A-Z]{0,3}\s*\d{1,6}\s?/\s?\d{2,4}\b",
                 )),
    # numer ksiegi wieczystej: XX0X0/########/#
    ("KW", _re(r"\b[A-Z]{2}\d[A-Z0-9]\d?/\d{6,8}/\d\b")),
    # kod pocztowy
    ("KOD", _re(r"(?<!\d)\d{2}-\d{3}(?!\d)")),
]

SKIP_KLUCZE = frozenset({
    # klucze infrastrukturalne protokolu - nie sa trescia uzytkownika.
    # UWAGA: "api_key" tu NIE jest - pole o tej nazwie w ciele zadania to czesto
    # klucz wklejony przez uzytkownika i MUSI byc zredagowane (bylo dziura do 2026-09-25).
    # UWAGA 2: "function" tu NIE ma (usuniete 2026-09-25) - pod tym kluczem siedza
    # ARGUMENTY wywolan narzedzi (tool_calls[].function.arguments) i to jest tresc,
    # ktora musi przejsc redakcje (sekret w historii narzedzi szedl na drut wprost).
    "model", "role", "type", "name", "id", "object", "finish_reason", "tool_call_id",
    "tool_choice", "stop", "stream", "user", "index", "created",
})

PH_RE = re.compile(r"\[([A-Z_?]{2,20})\s*#\s*(\d+)\]")


class Anonimizer:
    """Reguly strukturalne. Jedna instancja = jedno wywolanie API (jedna mapa)."""

    def __init__(self) -> None:
        self._mapa: Dict[str, str] = {}          # placeholder -> oryginal
        self._licznik: Dict[str, int] = {}       # TYP -> ile wystapien (unikalnych)
        self._licznik_hit: Dict[str, int] = {}   # TYP -> ile podstawien (z powtorzeniami)

    # ---------------- podmiana

    def _ph(self, typ: str, wartosc: str) -> str:
        for ph, orig in self._mapa.items():
            if orig == wartosc:
                self._licznik_hit[typ] = self._licznik_hit.get(typ, 0) + 1
                return ph
        n = self._licznik.get(typ, 0) + 1
        self._licznik[typ] = n
        ph = f"[{typ}#{n}]"
        self._mapa[ph] = wartosc
        self._licznik_hit[typ] = self._licznik_hit.get(typ, 0) + 1
        return ph

    def redact(self, tekst: str) -> Tuple[str, Dict[str, object]]:
        if not isinstance(tekst, str) or not tekst:
            return tekst, {"encje": {}, "podstawienia": {}, "znaki_in": 0, "znaki_out": 0}   # ten sam kształt co niżej (2026-09-28: KeyError na pustym napisie z Claude Code)
        t = unicodedata.normalize("NFC", tekst)
        out = t
        for typ, rex in WZORCE:
            wybrane = []
            for m in rex.finditer(out):
                a, b = m.span()
                # NIE wchodzimy w to, co jest juz placeholderem innej reguly:
                # inaczej SEKRET zjada "[KLUCZ_API#1]" razem z otoczeniem i restore
                # nie ma czego przywrocic (blokada: selftest_sekretow.py, api_key="sk-live-...").
                if any(p.start() < b and p.end() > a for p in PH_RE.finditer(out)):
                    continue
                frag = m.group(0)
                if typ == "PESEL":
                    if pesel_ok(frag):
                        wybrane.append((a, b, "PESEL", frag))
                    elif pesel_data_plausible(frag):
                        wybrane.append((a, b, "PESEL?", frag))
                    continue
                if typ == "NIP" and not nip_ok(frag):
                    continue
                if typ == "REGON" and not (_cyfry(frag) and (regon9_ok(frag) or regon14_ok(frag))):
                    continue
                if typ == "IBAN" and not iban_pl_ok(frag):
                    continue
                wybrane.append((a, b, typ, frag))
            # podmiana od konca: span-y wybranych zostaja wazne, numeracja w kolejnosci tekstu
            for a, b, typ2, frag in reversed(wybrane):
                out = out[:a] + self._ph(typ2, frag) + out[b:]
        return out, {
            "encje": dict(self._licznik),
            "podstawienia": dict(self._licznik_hit),
            "znaki_in": len(tekst),
            "znaki_out": len(out),
        }

    # ---------------- przywracanie

    def restore(self, tekst: str) -> str:
        if not isinstance(tekst, str) or not tekst:
            return tekst

        def sub(m: re.Match) -> str:
            ph = f"[{m.group(1)}#{m.group(2)}]"
            return self._mapa.get(ph) or m.group(0)

        return PH_RE.sub(sub, tekst)

    def stats(self) -> Dict[str, object]:
        return {"encje": dict(self._licznik), "podstawienia": dict(self._licznik_hit)}

    def __repr__(self) -> str:                   # NIGDY nie ujawnia mapy
        return f"<Anonimizer {self.stats()}>"


if __name__ == "__main__":
    a = Anonimizer()
    txt = ("PESEL 44051401359, NIP 1234563218, tel +48 601 234 567, mail jan.kowalski@example.com, "
           "sygn. akt II C 123/19, KW WA1M/00123456/3, kod 00-950.")
    r, s = a.redact(txt)
    print(r)
    print("stats:", s)
    print("restore OK:", a.restore(r) == txt)
