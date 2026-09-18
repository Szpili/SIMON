# Wideo zgłoszeniowe — scenariusz

**Długość docelowa: 2:30–3:00.** Format MP4, 1920×1080, 30 kl./s.

**Zasada nadrzędna: pokazujemy działający produkt, nie opowiadamy o nim.**
Najmocniejszy materiał, jaki mamy, to ekran, na którym weryfikacja realnie
odrzuca oszustwo. Tego nie da się podrobić animacją, a każde inne zgłoszenie
będzie miało animację.

**Nagrywamy PO pomiarze ROCm** — inaczej nagramy dwa razy.

---

## Przed nagraniem — lista kontrolna

- [ ] `systemctl --user is-active simon-node simon-node2 simon-demo` → trzy razy `active`
- [ ] `scripts/sprawdz_profile.sh` przechodzi (oba węzły odpowiadają)
- [ ] **Bielik rozgrzany** — pierwsze zlecenie po starcie Ollamy trwa ~2 minuty
      (ładowanie modelu). Puść jedno na sucho, żeby w nagraniu nie było ciszy.
- [ ] wyłączony wygaszacz Matrix i powiadomienia
- [ ] okno przeglądarki **dokładnie 1920×1080** (skrypt to sprawdza)
- [ ] czysty pulpit za oknem, bez cudzych danych na ekranie

**Uwaga o prywatności:** w pasku adresu widać nazwę maszyny i tailnetu. To ten
sam adres, który i tak podajemy w zgłoszeniu jako URL aplikacji, więc nie jest
to nowe ujawnienie — ale warto wiedzieć, że tam jest.

---

## Ujęcia

### 1. Plansza tytułowa — 0:00–0:12

Statyczna okładka (`okladka-1920x1080.png`), wjazd przez fade.

> **Napis / narracja:** SIMON — verifiable LLM inference on hardware you don't
> control. Every job comes back with a signed receipt anyone can check.

---

### 2. Zlecenie na pierwszym węźle — 0:12–0:45

Demo w przeglądarce. Wybrany węzeł **RTX 3090 · vLLM · Qwen3.8-27B**.
Wpisujemy zadanie, klikamy „Zleć zadanie", czekamy na wynik.

Pokazać: wynik, czas **zmierzony u klienta**, i rozwinięty receipt.

> **Napis:** The answer comes from a machine you do not control.
> The receipt is signed by that machine's Ed25519 key.

**Na co zwrócić kamerę:** linijka `policzył: vllm/0.27.1 · model qwen3.8-27b`.
Ona pochodzi **z podpisanego receiptu**, nie z opisu obok.

---

### 3. Przełączenie na inną kartę — 0:45–1:10

Zmieniamy węzeł na **RTX 3080 Ti · Ollama · Bielik 11B**, to samo zadanie.

Pokazać: ta sama strona, inny wynik, i **receipt mówi teraz `ollama/0.34.0`**.

> **Napis:** Different GPU, different serving stack, different model — one
> network. The engine is not a label we wrote: the node signed it.

**Dlaczego to ujęcie jest ważne:** dodanie drugiego węzła wykryło u nas dwa
fałszywe oświadczenia w receiptach. Jeden węzeł nigdy by ich nie pokazał.

---

### 4. Trzy próby oszustwa — 1:10–1:50

Sekcja „Nie wierz na słowo — sprawdź". Klikamy kolejno:

1. **Zweryfikuj receipt** → zielone, z rozbiciem na warstwy
2. **Podmień wynik** → `podpis Ed25519 nie pasuje`
3. **Podstaw pod inne zlecenie** → `podpis prawdziwy, ale to nie dowód na TĘ pracę`
4. **Podmień treść odpowiedzi** → `to nie jest ten wynik`

> **Napis przy czwartej próbie:** Until 18 September this attack passed every
> gate. The signature covered metadata, not the answer. We found it, fixed it,
> and wrote down what older receipts actually proved.

**To jest najmocniejsze ujęcie w całym wideo.** Nie skracać.

---

### 5. Weryfikacja offline w terminalu — 1:50–2:15

Terminal obok przeglądarki. Zapisany receipt i odpowiedź jako pliki.

```bash
simon --verify-receipt receipt.json --expect-output odpowiedz.txt
# podpis Ed25519 : OK
# treść wyniku   : zgodna z odciskiem
# werdykt        : RECEIPT WAŻNY

printf ' ' >> odpowiedz.txt          # jedna spacja więcej
simon --verify-receipt receipt.json --expect-output odpowiedz.txt
# treść wyniku   : NIEZGODNA — to nie jest ten wynik
# werdykt        : ODRZUCONY
```

> **Napis:** No network. No trust in whoever handed you the file.
> One extra space is enough to fail.

---

### 5b. To samo na macOS — 2:15–2:30  (opcjonalne, ale mocne)

**Nie** powtarzamy demo w przeglądarce — strona wygląda tam identycznie i nic
to nie dowodzi. Wartościowe jest jedno ujęcie: **terminal na MacBooku
weryfikuje ten sam receipt**, offline, na laptopie bez GPU.

```
user@macbook $ simon --verify-receipt receipt.json --expect-output odpowiedz.txt
podpis Ed25519 : OK
treść wyniku   : zgodna z odciskiem
werdykt        : RECEIPT WAŻNY
```

> **Napis:** The GPU was somewhere else. This laptop has none — and it still
> checks the work, with no network and no trust in the sender.

To domyka tezę architektoniczną projektu: **harness musi działać tam, gdzie
node nie wejdzie.** Klient z receiptem to plik tekstowy i jedna komenda.

**Czego wymaga to ujęcie, zanim je nagramy:**

- [ ] Pazur obudzony; Tailscale SSH wymaga tam **potwierdzenia w przeglądarce**,
      a konto to `user`, nie `trebusz` (objaw pomyłki:
      `failed to look up local user "trebusz"`)
- [ ] **przebudowana binarka** — ta na Macu pochodzi sprzed 18 września, więc
      nie ma ani wiązania treści, ani `--verify-receipt --expect-output`.
      Stara wersja pokazałaby weryfikację, która przepuszcza podmieniony tekst
- [ ] receipt i odpowiedź przeniesione na Maca jako zwykłe pliki — **to jest
      część przekazu**: nie ma kanału, nie ma sesji, są dwa pliki

Jeśli któregoś warunku nie da się spełnić na czas, **pomijamy to ujęcie**.
Lepiej nie pokazać macOS, niż pokazać starą binarkę robiącą słabszą
weryfikację, niż ta, którą opisujemy.

---

### 6. Wynik ROCm — 2:15–2:40  ⟵ **DO UZUPEŁNIENIA PO POMIARZE**

Notebook z AMD Developer Cloud albo wykres porównania CUDA ↔ ROCm.

> **Napis (wersja, gdy wyniki są zgodne):** An honest AMD node and an honest
> NVIDIA node produced the same activations. Fingerprints carry across vendors.
>
> **Napis (wersja, gdy się różnią):** They did not match. Naive fingerprint
> comparison would slash honest participants — which is exactly why we do not
> slash automatically.

**Obie wersje są dobrym materiałem.** Wynik negatywny jest nawet mocniejszy:
to realny problem całej dziedziny, a nie nasza porażka.

---

### 7. Status i zamknięcie — 2:40–3:00

Tabela statusu ze slajdów (ta bez zaokrąglania w górę), potem plansza końcowa.

> **Napis:** We do not slash on a threshold we cannot defend. The ledger records
> work, never a balance. Everything here is measured, and what is not measured
> says so.
>
> github.com/Szpili/SIMON · MIT

---

## Czego NIE nagrywać

- Sztucznie przyspieszonego demo. Bielik liczy wolno na 3080 Ti i **to jest
  prawdziwa informacja** o heterogenicznej sieci. Można skrócić montażem, ale
  z widoczną informacją, że to cięcie.
- Wygenerowanego wideo AI. Pokazałoby, że umiemy użyć cudzego modelu — czyli
  to samo, co każde inne zgłoszenie.
- Niczego, czego nie zmierzyliśmy. Jeśli coś jest deklaracją, napis ma to
  mówić.
