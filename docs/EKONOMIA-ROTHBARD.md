# SIMON — ekonomia wyprowadzona z Rothbarda

**Status: analiza / propozycja, nie decyzja i nie implementacja.** 2026-11-20.
English: [`EKONOMIA-ROTHBARD.en.md`](EKONOMIA-ROTHBARD.en.md).
Powiązane: [`EKONOMIA-STAKING.md`](EKONOMIA-STAKING.md), [`WASH-COMPUTE.md`](WASH-COMPUTE.md), [`THREAT-MODEL.md`](THREAT-MODEL.md).

**Źródło:** 75 nagrań wykładów i audiobooków Murraya Rothbarda (54,8 h, 494 769 słów), przetranskrybowanych lokalnie
(whisper large-v3-turbo). Serie: *Ethics of Liberty*, *For a New Liberty*, *History of Economic Thought*,
*What Has Government Done to Our Money* (dalej WHGDTOM) i zbiór pojedynczych wykładów. Ekstrakcja zasad: 8 workerów
LLM na pełnych transkryptach plus własna lektura rozdz. 2–3 WHGDTOM. **Recenzja:** GPT-6.2 Astra, werdykt
**CHANGES NEEDED**. Ten dokument zawiera już poprawki z tej recenzji.

## Wniosek w jednym akapicie

Rothbard daje SIMON-owi jedną twardą regułę: **dochód w sieci może pochodzić wyłącznie z dobrowolnej zapłaty
za zweryfikowaną usługę.** Emisja tokena dla węzłów to w jego języku fałszerstwo: przesuwa bogactwo do pierwszych
odbiorców nowego pieniądza (efekt Cantillona). Dokładnie to zabiło Golema, Helium i Filecoina, bo emisja
finansowała farmy, a nie produkt. Kredyt „na żądanie” bez zarezerwowanej mocy to rezerwa cząstkowa, a poranny szczyt byłby
na niej runem na bank. **MVP:** zlecenie w PLN, 100% przedpłaty, cenę ustala sprzedawca, nie ma tokena ani emisji.
Zamiana „noc → szczyt” pozostaje otwarta i trzeba ją zaprojektować jako uczciwy instrument z zabezpieczeniem.

## DLACZEGO — zasady i ich skutek dla SIMON

| Zasada Rothbarda (parafraza) | Gdzie | Skutek dla SIMON |
|---|---|---|
| Pieniądz nie powstaje z dekretu. Musi wyrosnąć z dobra już cenionego za bezpośredni użytek (twierdzenie regresyjne) | WHGDTOM, rozdz. 2 | Jedyna istniejąca wartość w sieci to zweryfikowane compute. Wyceniamy je w PLN. Drugi aktyw ma sens dopiero, gdy rynek sam zacznie go używać |
| Każda podaż pieniądza jest równie dobra. Dodruk niczego nie tworzy, tylko przesuwa bogactwo do pierwszych odbiorców | WHGDTOM, rozdz. 2–3 | Emisja epokowa dla kopaczy jest transferem od późnych uczestników do wczesnych. Zero emisji inflacyjnej to jawny wybór projektowy |
| Pokwitowanie na towar, którego nie ma w magazynie, to oszustwo. Bank z rezerwą cząstkową jest bankrutem, a widać to dopiero przy runie | WHGDTOM, rozdz. 2 | Compute jest nietrwałe: nocnych GPU-godzin nie da się przechować. Kredyt „wydaj rano, kiedy chcesz” bez rezerwacji to run o 9:00 |
| Prawo Greshama działa tylko przy wymuszonym parytecie (bimetalizm, legal tender) | WHGDTOM, rozdz. 3 | Liczenie prefilla z cache na równi z liczonym albo kwantyzacji na równi z pełną precyzją sprawia, że węzły dostarczają „startą monetę” |
| Umowa to przeniesienie tytułu, nie obietnica. Egzekwuje się tylko to, czego złamanie jest ukrytą kradzieżą | *Ethics of Liberty*, rozdz. 19 | Protokół egzekwuje escrow i kaucje, nie „gołe obietnice dostępności” |
| Kara proporcjonalna, najpierw restytucja | *Ethics*, rozdz. 13 | Najpierw zwrot klientowi, potem umówione odszkodowanie, na końcu kara z sufitem. Nie konfiskata „dla protokołu” |
| Homesteading: tytuł daje praca. Samo „odkrycie” go nie daje | *Ethics*, rozdz. 6–11 | Żadnego premine. Reputację i priorytet zdobywa się niezależnie opłaconą pracą |
| Konkurencyjne agencje arbitrażują. Dominująca agencja nie jest koniecznością (polemika z Nozickiem) | *Ethics*, rozdz. 29 | Protokół nie może zakazywać konkurencyjnych koordynatorów i weryfikatorów |
| Bojkot to nie agresja. Wartość jest subiektywna, a koszt nie wyznacza ceny | *Ethics*, rozdz. 18; *Demand and Supply* | Publiczne czarne listy są w porządku. Bilans „3 zł prądu vs 2,2 zł tokenów” to jeszcze nie wyrok. Wyrokiem byłby brak opłaconego odnowienia |

## Recenzja (GPT-6.2 Astra) — co zmieniła

| Reguła wstępnej wersji | Werdykt | Powód |
|---|---|---|
| regresja, zero emisji, 100% pokrycia, preferencja czasowa, Gresham, marka mennicy, kara, dochód z usługi, nieprzelewalność | **FIX** | Kierunek jest dobry, ale wstępna wersja była nieprecyzyjna. Rezerwacja to obietnica świadczenia, nie tytuł do aktywa (stąd dwie osobne księgi). Noc i ranek to dwie transakcje spot. Stała cena sprzedawcy przy ograniczonym zapasie jest legalna. Podpis nie dowodzi niezależności operatora ani poprawności liczników |
| brak legal tender, wartość subiektywna, brak demurrage | **KEEP** | — |
| „subsydiowane GPU = cykl koniunkturalny” | **DROP** | Austriacka teoria cyklu wymaga ekspansji kredytu i stopy procentowej. Tu jest tylko zależność od subsydium, którą trzeba zmierzyć |
| zakaz kartelu koordynatorów | **DROP** | Dobrowolne poolowanie jest legalne. Testuje się wyłączną kontrolę, zobowiązania i ostateczność rozliczeń |
| „marża protokołu = seigniorage / podatek” | **DROP** | Jawna marża na dobrowolnej usłudze nie jest podatkiem |

Precyzja: Helium ustala **wartość USD Data Credita**, a liczba DC za spalony HNT się zmienia. „Stały kurs
SIMON→THINK”, „stała wartość THINK w PLN” i „stała taryfa THINK za inferencję” to trzy różne zobowiązania.
Żadne z nich nie rezerwuje GPU.

**Trzy dziury wskazane w recenzji:**
1. **Weryfikator** opłacany tylko za akceptację traci na uczciwym odrzuceniu. Rozwiązanie: stała stawka za wykonaną
   recenzję przy każdym werdykcie. Weryfikatora przydziela się niezależnie od wykonawcy.
2. **Podwójna sprzedaż:** dwóch koordynatorów sprzedaje ten sam GPU na 9:00. Rozwiązanie: jedna księga przyjęć na urządzenie,
   limit ekspozycji operatora i zapas mocy w innej domenie awarii.
3. **Premia prywatności** znika u weryfikatora, bo on też widzi prompt. Rozwiązanie: dozwolony wykonawca **i** weryfikator
   wpisani do podpisanej oferty. Odrębna pula ma odrębną cenę.

## JAK — spec MVP (po korekcie)

1. **Jedno zlecenie w PLN.** Bez tokena, spalania, emisji epokowej, salda THINK i naliczania za bezczynne godziny.
2. **Podpisana oferta:** sprzedawca, hashe modelu i kwantyzacji, runtime, metoda weryfikacji, dozwoleni wykonawcy
   i weryfikatorzy, limity wejścia i wyjścia, termin, cena maksymalna, podział opłat, warunki porażki. Cenę ustala dostawca.
3. **Rozliczenie odtwarzalne:** na start stała cena za zlecenie w podpisanych limitach. `C = w_p·P + w_d·D` służy tylko
   jako wersjonowany benchmark, dopóki nie umie rozstrzygnąć sporu.
4. **100% finansowania przed wysyłką** u autoryzowanego dostawcy płatności. Codziennie uzgadnia się PLN klientów
   z PLN zabezpieczonymi. Przy niedoborze nowe zlecenia są zamrażane. Kaucje operatorów i pieniądze sponsora trzymane są osobno.
5. **Rezerwacja przed obietnicą:** twardy termin tylko z nazwaną mocą, remedium i zabezpieczeniem. Best-effort jest
   jawnie oznaczony.
6. **Jednokrotne rozliczenie** związane z unikalnym ID zlecenia. Weryfikator dostaje stałą stawkę przy każdym werdykcie.
   Ponowienia są idempotentne.
7. **Routing niezależny od wolumenu:** samodeklarowany obrót, liczba zleceń i oceny mają zerową wagę. To jest realna
   obrona przed wash-compute: przy zerowej emisji jedyną nagrodą za samozlecenia byłby lepszy routing.
8. **Skalowanie stopniowe:** 1 zlecenie od niepowiązanego klienta → odnowienie bez subsydium → 10–12 zróżnicowanych
   przypadków (odrzucenie, timeout, zwrot, kontencja szczytu) → dopiero setki.
9. **OTWARTE:** najmniejszy uczciwy instrument „noc → szczyt” (rezerwacja / forward na okno) przy 100% zabezpieczenia
   i zerowej emisji. Bez niego SIMON traci swoją jedyną wyróżniającą propozycję wartości.

## Prawo (bramki, nie porady)

Zlecenie płatne w PLN, zapisane w zwykłej bazie, z niezbywalnym pokwitowaniem ma mocne podstawy, żeby nie podpadać
pod definicję kryptoaktywa w MiCA (art. 3 ust. 1 pkt 5, motyw 17). Saldo akceptowane przez niezależne węzły zmienia
jednak tę analizę i otwiera też pytanie o pieniądz elektroniczny (dyrektywa 2009/110/WE, art. 2 ust. 2). **PIT nie ma
zwolnienia:** węzeł będący osobą fizyczną ma przychód z usług, także w naturze (prawo do compute). Te kwestie musi
rozstrzygnąć prawnik i doradca podatkowy, nie model.

## Czego nie zweryfikowano

- Notatki workerów LLM nie są sprawdzone zdanie po zdaniu. W całości przeczytano tylko rozdz. 2–3 WHGDTOM.
- Recenzent sprawdzał opublikowane teksty Rothbarda, nie nasze transkrypty.
- Liczby kosztowe (3 zł prądu za noc na 3090, 2,2 zł za tokeny) pochodzą z wcześniejszej roadmapy i nie zostały zmierzone na nowo.
