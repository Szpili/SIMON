# Dziennik bezpieczeństwa SIMON

Zapisujemy tu, **co było źle i od kiedy** — z opisem poprzedniego zachowania,
nie tylko poprawki. Czytelnik ma móc ocenić, co dokładnie dowodziły receipty
wystawione przed daną zmianą.

---

## 2026-09-18 — P0: receipt nie wiązał treści odpowiedzi

**Zachowanie przed poprawką.** `output_digest` był odciskiem METADANYCH:

```
signature → job_id + liczniki + timing
output    → poza podpisanym commitmentem
```

Sama odpowiedź szła obok receiptu, niepodpisana.

**Skutek.** Węzeł mógł nie policzyć nic i odesłać dowolny tekst: podpis Ed25519
pozostawał ważny, `job_id` i `model_hash` się zgadzały, więc wszystkie bramki
weryfikacji po stronie klienta zapalały się na zielono. To samo dotyczyło
podmiany w transporcie lub przez koordynatora oraz sklejenia receiptu z innego
wykonania z bieżącym tekstem.

**Co twierdziła dokumentacja.** Demo i whitepaper mówiły wprost, że podpis
obejmuje odcisk wyniku. **To zdanie było nieprawdziwe.**

**Poprawka.**

```
signature → receipt zawierający commitment do outputu
client    → przelicza commitment z odebranej treści
```

- `output_digest` = `H("SIMON/OUTPUT/v1" ‖ job_id ‖ output)` — separator domeny
  i związanie ze zleceniem, więc poprawnego receiptu nie da się podstawić pod
  ten sam tekst w innym zleceniu;
- klient dostał czwartą bramkę: przelicza odcisk z tego, co realnie odebrał;
- `--verify-receipt --expect-output <plik>` pozwala to sprawdzić offline komuś,
  kto dostał wynik i podpis od osoby trzeciej;
- testy regresyjne na dokładnie ten atak (podmiana treści, podmiana o jeden
  znak, podbicie liczników po podpisaniu, separator domeny).

**Co ta poprawka zamyka:** podmianę odpowiedzi przez węzeł, podmianę
w transporcie lub przez koordynatora, sklejenie receiptu z innego wykonania
z bieżącym tekstem, modyfikację wyniku po podpisaniu.

**Czego NIE zamyka:** nie dowodzi, że zadeklarowany model wygenerował podpisaną
treść. Złośliwy węzeł może podpisać dowolny tekst wraz z poprawnie policzonym
odciskiem. Dopiero audyt wykonania ma związać `model + prompt + wykonanie + output`.

**Receipt po poprawce dowodzi dokładnie tego:**
> Zarejestrowany węzeł podpisał dokładnie tę odpowiedź jako wynik tego zlecenia.

**I nadal NIE dowodzi:**
> Zadeklarowany model wygenerował tę odpowiedź.

---

## 2026-09-18 — czasy: odporności na manipulację NIE MA

**Zachowanie przed poprawką.** `gen_ms` liczono jako
`tokens_out / 65.0 * 1000` — ze stałej zmierzonej kiedyś dla Bielika 11B na
3080 Ti, stosowanej do KAŻDEGO węzła i modelu. Komentarz obok twierdził, że to
„twarda miara, nie heurystyka tok/s", czyli dokładnie odwrotnie niż robił kod.

**Skutek.** Przepustowość liczona z tego pola wychodziła **zawsze 65,0 tok/s**,
niezależnie od sprzętu i modelu. Demo pokazywało tę liczbę jako pomiar.

**Poprawka.** `gen_ms` to realnie zmierzony czas wywołania backendu (bez
streamingu obejmuje prefill i dekodowanie razem — tego się nie da rozdzielić).
Po poprawce ten sam prompt daje 910 ms na vLLM/3090 i 7798 ms na Ollamie/3080 Ti.

### Model zagrożenia: manipulacja czasem

**Nie mamy żadnej odporności i nie da się jej uzyskać samymi podpisami.**

| Warstwa | Kto mierzy | Co gwarantuje podpis | Czy da się skłamać |
|---|---|---|---|
| `node_declared_ttft_ms`, `node_declared_gen_ms` | węzeł o sobie | tylko brak zmiany po fakcie | **tak, dowolnie** |
| `ClientObservationV1` | klient o sobie | tylko brak zmiany po fakcie | **tak, dowolnie** |
| różnica klient/węzeł | dwie strony o przeciwnych interesach | nic | **tak, przy zmowie** |

Dodatkowo `started_at_us` / `finished_at_us` w receipcie pochodzą z zegara
ściennego węzła — ustawialnego dowolnie, a przy NTP także skaczącego. Klient
używa zegara **monotonicznego**, więc jest odporny na skoki, ale nie na kłamstwo.

**Po co ktoś miałby manipulować czasem:** żeby wyglądać szybciej i wygrywać
przydział, żeby wyglądać wolniej i uzasadnić wyższą opłatę, albo żeby nadać
wiarygodny czas trwania fikcyjnej pracy przy wash-compute.

**Czego wymagałaby realna odporność.** Czasu nie da się dowieść kryptograficznie
między stronami, które sobie nie ufają. Przybliżenia: atestacja sprzętowa (TEE),
niezależni obserwatorzy próbkujący węzeł (statystyka, nie dowód), albo
wyprowadzenie oczekiwanego czasu z zaudytowanego wykonania i znanego profilu
sprzętu — nadal zależne od sprzętu.

**Stanowisko praktyczne, obowiązujące od teraz:** czas NIE jest podstawą
rozliczeń, slasha, rankingu wydajności ani rozstrzygania sporów. Służy wyłącznie
jako sygnał operacyjny. Rozliczenie ma się opierać na licznikach tokenów
(dopiero gdy będą niezależnie przeliczalne — M5.1b) i na audycie wykonania.

## 2026-09-18 — audyt wszystkich stałych

Wniosek po incydencie ze stałą `65.0`: jeżeli jedna liczba udająca pomiar
przeżyła kod, komentarz i demo, to nie należy zakładać, że jest jedyna.
Przegląd wszystkich stałych w `crates/`, każda zaklasyfikowana.

| Stała | Miejsce | Klasyfikacja | Działanie |
|---|---|---|---|
| `tokens_out / 65.0 * 1000` | `node.rs` | **FAKE_MEASUREMENT** | usunięta — realny pomiar |
| `ZNAK_NA_TOKEN_WORST_CASE = 1.2` | `mapreduce.rs` | CALIBRATION_CONSTANT | zostaje — ma źródło, datę i jawne „to nie jest gwarancja formalna" |
| `ZNAK_NA_TOKEN_PROZA_GENEROWANA = 3.5` | `mapreduce.rs` | CALIBRATION_CONSTANT | zostaje — źródło + incydent, który ją wymusił |
| `narzut_szablonu_tokenow = 60` | `agent.rs` | **ZAŁOŻENIE bez źródła** | nazwane `NARZUT_SZABLONU_TOKENOW`, opisane jako założenie wraz z konsekwencją pomyłki |
| `empiryczny * 0.9` | `agent.rs` | margines bez nazwy | nazwane `MARGINES_ZAOSTRZENIA` |
| `chars / 3.5` (inline) | `agent.rs` | duplikat stałej | zastąpione nazwaną stałą — groziło rozjazdem |
| `max_tokens 128`, `chunk_tokens 4000`, `map_max_tokens 200` | `main.rs` | CONSTANT_CONFIG | zostają — jawne domyślne wartości CLI |
| `TIMEOUT_ODPOWIEDZI 300 s`, `REQUEST_TIMEOUT` | `agent.rs`, `harness` | CONSTANT_LEGITIMATE | zostają |

Przemianowane też pole `tok_s` → `node_declared_tok_s`: liczone jest z czasu
zadeklarowanego przez węzeł, więc nazwa musi to mówić.

### Test, którego brakowało

Błąd przeszedł przez kod, komentarz i demo, mimo że **sygnał był widoczny**:
dwie różne karty pokazywały jedną liczbę. Brakowało testu na podejrzanie stałą
wartość. Dodane:

- `simon_core::pomiar::podejrzanie_identyczna` + testy na danych historycznych:
  para (65,0; 65,0) jest łapana, para zmierzona po poprawce (44,0; 3,3) przechodzi;
- `scripts/sprawdz_profile.sh` — inwariant na ŻYWYCH węzłach: dwa różne profile
  nie mogą dać tej samej przepustowości, a węzeł nie może zadeklarować czasu
  DŁUŻSZEGO niż zmierzony u klienta. Przebieg 2026-09-18:
  vLLM/3090 64,94 tok/s, Ollama/3080 Ti 6,50 tok/s — inwariant spełniony.

Uwaga na marginesie, warta zapamiętania: stała 65 była **mniej więcej trafna
dla jednego węzła** (zmierzone 64,94 tok/s). Właśnie dlatego nikt jej nie
zakwestionował — fałszywy pomiar, który zgadza się w jednym przypadku, jest
trudniejszy do wykrycia niż oczywisty nonsens.

### Zegar ścienny zdegradowany

`Receipt::elapsed_secs()` liczyło czas trwania z różnicy `started_at_us`
i `finished_at_us` — dwóch znaczników z zegara ściennego węzła. Poza tym, że
węzeł może je ustawić dowolnie, NTP potrafi skokowo cofnąć czas, więc różnica
bywa losowa albo (przez `saturating_sub`) wyzerowana, co maskuje problem.
Metoda jest teraz oznaczona jako `#[deprecated]`, a pola zostają wyłącznie jako
orientacyjny znacznik do logu. **Czas trwania mierzy się zegarem monotonicznym
po stronie, która mierzy. Nigdy nie odejmujemy znaczników z dwóch maszyn.**

### Hierarchia sygnałów czasu

```
client_observed_total_ms   — najsilniejszy dostępny sygnał
node_reported_gen_ms       — słaby, wyłącznie diagnostyczny
node wall-clock timestamps — najsłabszy, do niczego wiążącego
```

Czasy węzła są nawet słabsze niż obserwacja klienta, bo dotyczą wnętrza
procesu, którego nikt z zewnątrz nie obserwuje. Klient mierzy przynajmniej
realny czas transportu.

## 2026-09-18 — tożsamość klienta była efemeryczna

**Zachowanie przed poprawką.** Agent wołał `Keypair::generate()` przy KAŻDYM
uruchomieniu. Dwa uruchomienia były dwiema różnymi osobami.

**Skutek.** Metrycznik M5.2 — ogłoszony poprzedniego dnia jako zrobiony —
**nie potrafił przypisać pracy do nikogo**: pole `client_pubkey` było w każdym
rekordzie innym losowym kluczem. Dowód z żywego pliku: 2 rekordy, 2 klucze.
Błąd znalazł się dopiero przy pytaniu o odzyskiwanie tożsamości: okazało się,
że nie ma czego odzyskiwać.

**Poprawka (M5.2a).** `simon_core::tozsamosc`: trwały klucz w katalogu danych
użytkownika, zapis atomowy (plik tymczasowy w tym samym katalogu → `fsync` →
`0600` → `rename` → ponowny odczyt → porównanie klucza publicznego),
`--identity-file` do jawnego wskazania innej ścieżki.

**Reguła, która jest tu najważniejsza: nigdy nie regenerujemy klucza po
błędzie.** Uszkodzony plik, zbyt szerokie prawa albo brak możliwości zapisu
**zatrzymują start**. Cicha regeneracja wyglądałaby jak udany start, a po cichu
tworzyłaby nową tożsamość i znowu rozcinała metrycznik — ten sam błąd, tylko
trudniejszy do zauważenia.

Sekret nie jest przyjmowany w wierszu poleceń ani w zmiennej środowiskowej
(byłby widoczny w `ps` — patrz incydent z `--key` tego samego dnia) i nie
trafia do żadnego logu. `Keypair` celowo nie wystawia sekretu i nie ma `Debug`.

**Stare rekordy zostają nietknięte.** Niosą `identity_epoch: 0` =
`LEGACY_EPHEMERAL_IDENTITY`, `ownership: UNRECOVERABLE`. Dowodzą, że konkretny
efemeryczny klucz podpisał pracę, ale nowa tożsamość nie może kryptograficznie
udowodnić, że kontrolowała tamte klucze — **więc ich nie przepisujemy**.

**Zweryfikowane na żywo:** 2 zlecenia → 1 klucz klienta (przed: 2 → 2), plik
`0600`, zero trafień sekretu w wyjściu agenta. Osiem testów jednostkowych,
w tym: uszkodzony plik nie powoduje cichej regeneracji, przerwany zapis nie
niszczy poprzedniego klucza, zbyt szerokie prawa zatrzymują start, komunikat
błędu nie zawiera sekretu.

**Czego tu NIE MA (świadomie):** frazy odzyskiwania, rotacji, unieważniania
i wyprowadzania wielu ról z jednego korzenia. To wymaga zamrożonego formatu
i wektorów testowych — M5.2b/M5.2c w `ROADMAP.md`.

## 2026-09-18 — zmiana formatu odcisku unieważniła starsze receipty

Dodanie separatora domeny `SIMON/OUTPUT/v1` zmieniło sposób liczenia
`output_digest`. Receipty wystawione wcześniej tego samego dnia **nie
przechodzą** weryfikacji obecną binarką — i **słusznie**, bo commitment liczy
się inaczej.

**Problem nie w zachowaniu, tylko w komunikacie.** Receipt nie niesie wersji
schematu, więc stary receipt jest odrzucany jako „treść niezgodna", podczas gdy
prawdziwym powodem jest nieznany format. Ktoś diagnozujący to bez tej notatki
szukałby podmiany treści, której nie było.

Złapane przy wkładaniu przykładowego receiptu do repo: przykład nie przeszedł
własnej weryfikacji. Naprawione świeżym receiptem; **wersjonowanie schematu
zostaje jako M5.1c** (`ReceiptV2` ma nieść `schema`, a stary format ma dostać
status `LEGACY_OUTPUT_BOUND`, nie cichy błąd treści).

## Status weryfikacji — stan na 2026-09-18

```
Output binding:          ZROBIONE + TESTY
Token fields signed:     ZROBIONE
Token counts recomputed: CZĘŚCIOWO / do sprawdzenia osobno
Execution correctness:   NIEZROBIONE
Economic credit:         NIEZROBIONE
Wash-compute resistance: NIEROZWIĄZANE
Node timing signed:      ZROBIONE
Node timing true:        NIEDOWIEDZIONE
Client total time:       MIERZONE (od 2026-09-18)
Client TTFT:             TYLKO ZE STREAMINGIEM (brak)
Timing used economically: ZAKAZANE
Receipt schema split:    ODŁOŻONE DO M5.1c
Audyt stałych:           ZROBIONY (1 FAKE_MEASUREMENT, 3 nienazwane)
Test na stałe pomiary:   ZROBIONY (jednostkowy + na żywych węzłach)
Wall-clock jako czas:    ZDEGRADOWANY do logu (#[deprecated])
Tozsamosc klienta:       TRWALA (M5.2a)
Odzyskiwanie tozsamosci: NIEZAPROJEKTOWANE (M5.2b/c)
Korelacja klienta:       MVP_IDENTITY, prywatnosc niezaprojektowana
```

### Znane ograniczenia, nazwane wprost

**Liczniki tokenów są deklaracją węzła.** `prompt_tokens` i `completion_tokens`
są podpisane, więc nie da się ich zmienić po fakcie — ale nikt ich niezależnie
nie przeliczył. Docelowy kontrakt: klient wysyła `input_token_ids`
i `prompt_commitment`, węzeł zwraca `ordered_output_token_ids`, a klient
sprawdza `len(token_ids) == completion_tokens` oraz `decode(token_ids) == text`.
Dziś żaden z naszych backendów nie wystawia `token_ids` przez API OpenAI.

**Commitment jest po zdekodowanym tekście, nie po `token_ids`.** Różne
sekwencje tokenów mogą zdekodować się do tego samego tekstu, a weryfikacja
inferencji dotyczy sekwencji faktycznie przetworzonej przez model. Commitment
docelowy ma objąć także `finish_reason`, tool calle z argumentami, wyniki
multimodalne, wariant przy `n > 1`, manifest modelu i tokenizera, parametry
samplingu i wersję schematu receiptu.

**Czasy `ttft_ms` i `gen_ms` nie są zaufane.** Pochodzą od węzła; podpis
uniemożliwia ich późniejszą zmianę, ale nie czyni ich prawdziwymi. Niezależnie
mierzony jest wyłącznie czas obiegu po stronie klienta. **Rozliczenia nie mogą
opierać się na czasie zgłoszonym przez węzeł.**

**Serializacja nie jest kanoniczna w sensie RFC 8785.** `content_digest` używa
`serde_json::to_string`. Powtarzalne dla tej implementacji (mapy to `BTreeMap`,
floaty w podpisywanej treści zakazane od 2026-09-17), ale druga implementacja
protokołu może policzyć inny strumień bajtów.

### Testy zabijające — stan pokrycia

| # | Test | Stan |
|---|---|---|
| 1 | podmiana jednego bajtu tekstu | **jest** |
| 2 | podmiana jednego token ID | niewykonalny — brak token IDs w commitmencie |
| 3 | ten sam tekst, inna sekwencja token IDs | niewykonalny — jw. |
| 4 | podmiana kolejności tool calli | niewykonalny — poza commitmentem |
| 5 | podmiana argumentu tool calla | niewykonalny — jw. |
| 6 | podmiana `finish_reason` | niewykonalny — jw. |
| 7 | receipt z joba A z outputem joba B | **jest** |
| 8 | ten sam output, inny prompt commitment | niewykonalny — brak prompt commitment |
| 9 | zawyżone `completion_tokens` | **częściowo** — łapiemy zmianę PO podpisie, nie kłamstwo przy podpisie |
| 10 | zawyżone `prompt_tokens` | **częściowo** — jw. |
| 11 | inny tokenizer revision | niewykonalny — brak manifestu tokenizera |
| 12 | ponowne użycie receiptu (replay) | niewykonalny — brak rejestru |

Pozycje „niewykonalny" nie są odłożone z wygody: każda wymaga pola, którego
protokół dziś nie ma. Są rozpisane w M5.2 w `ROADMAP.md`.
