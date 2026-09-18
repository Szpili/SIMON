# Sprawdź sam, bez uruchamiania sieci

Prawdziwy receipt z węzła SIMON i odpowiedź, której dotyczy. Weryfikacja jest
**offline** — bez sieci i bez zaufania do tego, kto Ci te pliki podał.

```bash
cargo build --release
./target/release/simon --verify-receipt przyklady/receipt.json \
                       --expect-output przyklady/odpowiedz.txt
```

Powinno wypisać `RECEIPT WAŻNY` i zakończyć się kodem 0.

Teraz spróbuj oszukać:

```bash
cp przyklady/odpowiedz.txt /tmp/podmieniona.txt
printf ' ' >> /tmp/podmieniona.txt        # JEDNA spacja więcej

./target/release/simon --verify-receipt przyklady/receipt.json \
                       --expect-output /tmp/podmieniona.txt
# treść wyniku : NIEZGODNA — to nie jest ten wynik
# werdykt      : ODRZUCONY        (kod wyjścia 1)
```

## Czego to dowodzi, a czego nie

**Dowodzi:** zarejestrowany węzeł podpisał **dokładnie tę odpowiedź** jako wynik
**tego zlecenia**. Podmiana treści, podmiana w transporcie i podstawienie
receiptu z innego wykonania są odrzucane.

**NIE dowodzi:** że zadeklarowany model ją wygenerował. Złośliwy węzeł może
podpisać dowolny tekst wraz z poprawnie policzonym odciskiem. Do związania
`model + prompt + wykonanie + output` potrzebny jest audyt wykonania, którego
**nie ma** — patrz `docs/SECURITY-LOG.md`.

Liczniki tokenów w receipcie to **deklaracja węzła**: podpis uniemożliwia ich
późniejszą zmianę, ale nie czyni ich prawdziwymi.

## Uwaga o zgodności

Receipty wystawione **przed 18 września 2026** nie przejdą tej weryfikacji.
Odcisk wyniku dostał wtedy separator domeny (`SIMON/OUTPUT/v1`), więc format
commitmentu się zmienił. Receipt nie niesie jeszcze wersji schematu, więc stary
receipt jest odrzucany z komunikatem „treść niezgodna" zamiast „nieznana wersja"
— to jest **znany dług**, rozpisany jako M5.1c w `ROADMAP.md`.
