# SIMON — konsylium: ekonomia i ścieżki rozwoju

**Status: analiza / rekomendacja, nie decyzja.** 2026-11-20.
English: [`EKONOMIA-KONSYLIUM.en.md`](EKONOMIA-KONSYLIUM.en.md). Kontynuacja: [`EKONOMIA-ROTHBARD.md`](EKONOMIA-ROTHBARD.md).

**Kolejność głosów:** zasady z Rothbarda (Claude Opus) → recenzja GPT-6.2 Astra → recenzja Gemini 4 Neon (+ uruchomienie
jego symulacji) → konsylium 7 modeli (GPT/codex, DeepSeek ×2, Gemini, cline-free, Cursor jako ekspert od przepisów
płatniczych KNF, GPT-5.6 Sol) → Argon (Gemini, osobna odpowiedź).

## Wniosek

**Bez tokena, escrow, sald i kaucji. Przez 30 dni sprzedajemy tylko płatny benchmark lokalnej inferencji (F) na danych bez
RODO, za co najmniej 2000 zł netto, na zwykłą fakturę. Rozmowy sprzedażowe rozstrzygają, czy istnieje produkt A1 czy A2.
Kod sieciowy jest zamrożony na 30 dni.** Dwóch sceptyków (oba DeepSeeki) odmówiło wyboru ścieżki, bo nie ma żadnych danych
o popycie. To jest uczciwy wniosek: każda ścieżka to dziś hipoteza.

## Co obalono w poprzednich ustaleniach

| Twierdzenie | Kto obalił | Dlaczego |
|---|---|---|
| „100% przedpłaty w PLN u PSP” załatwia sprawę pieniędzy (Opus, Astra, Neon) | ekspert KNF, GPT-5.6 Sol, codex | Escrow, kaucja i wypłata po werdykcie to w praktyce usługa płatnicza (art. 3 ustawy o usługach płatniczych), czyli ścieżka do licencji MIP/KIP. Saldo przedpłacone w bazie to też przechowywanie cudzych środków. Bezpieczne warianty, **do prawnika**: SIMON jako *merchant of record* płacący operatorom fakturą B2B, każdy operator jako osobny sprzedawca albo marketplace pod licencją PSP |
| Forward na okno z kaucją κ ≥ 1,5 odstrasza podwójną sprzedaż (Neon) | KNF + rachunek | Kaucje wielu stron to zarządzanie cudzymi środkami. Poza tym przy niskim popycie kaucja nie odstrasza (tabela niżej) |
| Jedna księga przyjęć na urządzenie blokuje podwójną sprzedaż | codex, GPT-5.6 Sol | Bez odpornej na Sybil atestacji sprzętu operator zgłosi tę samą kartę pod wieloma kluczami albo sprzeda ją poza SIMON |
| Zaufana pula z NDA daje prywatność (Neon) | codex, GPT-5.6 Sol, DeepSeek | NDA daje możliwość dochodzenia roszczeń, a nie poufność techniczną. Operator staje się podmiotem przetwarzającym w rozumieniu RODO (DPA, retencja, logi) |
| Ścieżka D: weryfikowalna inferencja | GPT-5.6 Sol, codex | Dziś to obietnica nieprawdziwa marketingowo. Wynik jest „podpisany przez operatora”, nie „zweryfikowany”, dopóki nie zmierzono fałszywych akceptacji i odrzuceń TopLoc na różnych GPU |
| Canary jobs rozwiązują problem weryfikatora (Neon) | GPT-5.6 Sol, DeepSeek | Weryfikator może tylko próbkować. Bez zmierzonego kosztu pełnej weryfikacji nie ma dowodu, że oszustwo ma ujemną wartość oczekiwaną |
| Zgodność z Rothbardem jako argument za ścieżką (Opus) | DeepSeek ×2, GPT-5.6 Sol | Zasada normatywna to nie wykonalność rynkowa. Zero emisji nie tworzy popytu |

**Z Rothbarda zostaje** to, co konsylium potwierdziło argumentami regulacyjnymi i operacyjnymi: brak tokena (jednogłośnie),
dochód wyłącznie z zapłaty za usługę i brak premine.

### Podwójna sprzedaż — poprawny rachunek

Pierwsza analiza symulacji Neona użyła złej metryki: p95 pojedynczego losowania (wytknął to Argon). Poprawnie trzeba porównać
wartość oczekiwaną oszusta, który wystawia slot u 3 koordynatorów, z uczciwym operatorem u jednego. Model zakłada 100%
wykrywalności kolizji.

| Popyt p | Uczciwy | κ = 0,5 | κ = 1,0 | κ = 1,5 | κ = 2,5 |
|---|---|---|---|---|---|
| 0,2 | 0,170 | **0,359** | **0,303** | **0,247** | 0,135 |
| 0,35 | 0,297 | **0,454** | 0,292 | 0,130 | −0,195 |
| 0,5 | 0,425 | **0,431** | 0,119 | −0,194 | −0,819 |

Pogrubione: podwójna sprzedaż się opłaca. Przy niskim popycie, czyli w młodej sieci, opłaca się nawet przy κ = 1,5.

## Ranking ścieżek (GPT-5.6 Sol; mianownik: 100 podobnych prób)

| # | Ścieżka | P(≥ 1000 zł / 90 dni) | Kod do 1. sprzedaży |
|---|---|---|---|
| 1 | A. Zaufana pula jako płatny pilotaż B2B | 35/100 | 2–4 tyg. |
| 2 | F. Płatny benchmark/audyt prywatnej inferencji | 30/100 | 1–2 tyg. |
| 3 | C. Hosting jednej niszowej wagi dla konkretnego klienta | 18/100 | 3–6 tyg. |
| 4 | E. Badania / open source | 10/100 | mało |
| 5 | D. Weryfikowalna inferencja | 5/100 | 2–6+ mies. |
| 6 | B. Publiczny rynek slotów | 3/100 | 3–6+ mies. |

To ocena ekspercka jednego modelu, nie dane.

## Argon — korekta planu A + F

Werdykt: **plan konsylium wymaga korekty.** TAK dla porzucenia tokena, escrow i kodu przed sprzedażą. **VETO dla ścieżki A
w formie „domowe GPU + RODO/DPA”.**

1. **Domowe GPU są dla klienta z wymogiem RODO wadą.** CTO lub IOD wybierze dedykowane GPU w UE z ISO 27001 i jednym DPA,
   chmurę EU z zerową retencją albo własną skrzynkę, a nie łańcuch podpowierzeń do prywatnych mieszkań. A trzeba rozbić na:
   - **A1:** dane wrażliwe na on-prem klienta albo jednym DC w UE, SIMON jako warstwa audytowa i gateway (podpisane receipty, failover).
     Uczciwie: to już nie sieć domowych GPU, tylko oprogramowanie lub usługa.
   - **A2:** 1–3 znane domowe GPU **wyłącznie dla danych bez RODO** (agenci, batch, ewaluacje, wagi niszowe).
2. **Pilot za 1000–2500 zł z DPA ma ujemną marżę** (lejek 25–40 h, przegląd DPA u prawnika 1500–3000 zł; wszystko to
   założenia). Uwaga: przy 40 h i 2000 zł nawet F wychodzi na 2 h/100 zł, czyli na granicę kryterium zabicia.
3. **F waliduje doradztwo, nie SIMON** (fałszywy pozytyw: „dzięki, postawimy u siebie”). Dlatego w każdej rozmowie trzeba
   pytać, kto i za ile ma to potem utrzymywać, i testować skłonność do zapłaty za podpisany ślad wykonania
   („+20% za receipt z hashem modelu czy wystarczy log HTTP?”).
4. **F bez danych osobowych.** Zastrzeżenie: dane „zanonimizowane przez klienta” bywają tylko pseudonimizowane, a te nadal
   są danymi osobowymi. Bezpieczniej brać wyłącznie dane publiczne lub syntetyczne. **Do prawnika.**

## Plan 30 dni (po wszystkich głosach)

1. **Produkt:** benchmark lokalnej inferencji (2–3 otwarte modele vs obecne API klienta), dane publiczne lub syntetyczne,
   ≥ 2000 zł netto, faktura, płatność bezpośrednio do sprzedawcy. Każda odpowiedź ma receipt Ed25519 (hash wag, kwantyzacja,
   runtime, TTFT, tok/s), opisany jako „podpisany ślad wykonania”, nigdy „zweryfikowany”.
2. **Lejek:** 30 kontaktów, 15 z bólem prywatności i compliance (test A1), 15 z bólem burstu i wag niszowych (test A2).
3. **Bramka dnia 14:** mniej niż 5 umówionych rozmów na 30 kontaktów → zmiana komunikatu lub kanału, nie kod.
4. **Bramka dnia 30:** 0 opłaconych F → STOP komercji, zostaje ścieżka E. F się sprzedaje, ale 0/5 chce cyklicznego
   utrzymania → to firma konsultingowa, nie produkt sieciowy.
5. **Zamrożenie kodu:** jedyny nowy kod to runner benchmarku (metryki, receipty, twarde limity tokenów przeciw OOM).

## YAGNI na ten etap

Publiczny order book, VRF, kaucje, canary jobs, forwardy, escrow, wielu koordynatorów, routing reputacyjny, koszyki S/M/L
(przed rozkładem ~1000 realnych zleceń), symulacje wash-compute, token i MiCA.

## Czego nie zweryfikowano

Ranking i koszty (lejek, prawnik, ceny DC) to oceny i założenia modeli, nie dane. Kwestie płatnicze, RODO i podatkowe
wymagają opinii prawnika. Tabela podwójnej sprzedaży jest dokładna dla modelu Neona (3 koordynatorów, 100% wykrywalności
kolizji), nie dla rzeczywistego rynku.
