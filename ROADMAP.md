# ROAD MAPA — SIMON (stan 2026-09-16, po recenzji Opusa 5)

**Stan faktyczny:** 51 testów zielonych, rdzeń + protokół klient↔koordynator gotowe.
**Werdykt Opusa:** „to połowa drogi" (brak transportu danych) — **i głębiej: brak
weryfikacji obliczeń.** Kod sprawdza dziś tylko integralność podpisu.

---

## PLAN NA PAŹDZIERNIK — AMD Developer Hackathon: ACT III (dopisane 2026-09-18)

**Twardy termin: 18 października, 16:00 CEST** (koniec zgłoszeń). Faza online
12–18.10. Zespół: SIMON, zapisany. Repo publiczne, MIT:
`github.com/Szpili/SIMON`. Demo żywe.

### Ścieżka krytyczna — jedna, i wszystko inne przy niej blednie

```
wniosek o kredyt AMD  ──(2-3 dni robocze)──>  pomiar ROCm  ──>  wynik w README + whitepaper
```

Pomiar międzyvendorowy jest **jedyną częścią zgłoszenia, która naprawdę
wymaga ich sprzętu**, i jedyną, której nikt inny nie ma. W kryterium
„Application of Technology" waży najwięcej. Wszystko poza tą ścieżką da się
robić równolegle i da się skrócić; tej nie.

Kredyt wygasa **30 dni od aktywacji**, nie od przyznania — więc wniosek
składamy od razu, a aktywujemy dopiero, gdy notebook ma na czym pójść.

### Tydzień 1 (18–24.09) — odblokowanie

- [ ] **wniosek o kredyt AMD** — `developer.amd.com` → Member Perks → formularz.
      Wybór: **AMD Developer Cloud ($100)**, nie Fireworks ($50): pomiar
      potrzebuje kontroli nad stosem, na zarządzanym endpointcie mierzylibyśmy
      cudzą czarną skrzynkę
- [ ] **ziomki rejestrują się w AMD AI Developer Program** — to warunek
      dopuszczenia do ACT III, nie formalność. Bez tego nie liczą się jako zespół
- [ ] **`qwen-serving.service`** — dziś vLLM chodzi jako ręczny proces.
      Po restarcie Szpona demo wstanie i będzie zwracać błędy: usługi pokażą
      „active", strona będzie martwa. Najgorszy rodzaj awarii, bo wygląda dobrze
- [ ] rozmowa z autorem Vulkana: co dokładnie ma na CUDA. Vulkan compute chodzi
      na Radeonach **bez ROCm** — to druga, niezależna droga do zgodności z AMD,
      szersza sprzętowo choć wolniejsza

### Tydzień 2 (25.09–01.10) — pomiar

- [ ] **ramię A na ROCm** — `notebooks/SIMON-D2-ROCm.ipynb`, sesja JupyterLab
      (darmowa, 3 h/dobę). Baza CUDA jest w środku, werdykt wychodzi w sesji
- [ ] **ramiona B–E (vLLM)** na VM 1× MI300X — to, co padło na Franku przez brak
      UVA pod WSL2. Obraz Quick Start ma ROCm i vLLM gotowe
- [ ] wynik do `DESIGN-VERIFIER-0.md` i README — **także jeśli wyjdzie
      niewygodnie**. Wynik „AMD ≠ NVIDIA mimo uczciwości obu stron" jest
      mocniejszym materiałem niż wynik potwierdzający, bo to realny problem
      całej dziedziny, nie nasza porażka
- [ ] VM **ZNISZCZYĆ** po pomiarze. Wyłączenie nie zatrzymuje naliczania

### Tydzień 3 (02–08.10) — demo, które przeżyje ocenę

Sędziowie klikają **po 18 października**, a dziś wszystko stoi na jednym
domowym komputerze pod adresem zdradzającym nazwę maszyny i tailnetu.

- [ ] drugi węzeł (Pazur / Franko) — demo pokazujące **różny sprzęt w jednej
      sieci** dowodzi tezy projektu lepiej niż jakikolwiek opis
- [ ] rozważyć węzeł na AMD przez Fireworks: endpoint zgodny z OpenAI, 90 dni
      ważności — jedyny sposób, żeby po hackathonie demo dalej stało na AMD
- [ ] własna domena zamiast adresu `.ts.net` z portem (porty niestandardowe
      bywają blokowane w sieciach firmowych — sędzia zobaczy wtedy pustkę)
- [ ] przejrzeć przepustnicę pod kątem dnia oceny: 40/godz. może być za mało,
      gdy kilku sędziów klika naraz, i za dużo, gdy znajdzie to bot

### Tydzień 4 (09–18.10) — zgłoszenie

- [ ] **12.10: tracki zostają ogłoszone** — do tej pory ich nie znamy. Plan nie
      może zakładać, że SIMON w nie trafi; jeśli nie trafi, przepisujemy opis
      pod trak, nie projekt pod trak
- [ ] wideo (MP4) + slajdy (PDF) + okładka 16:9 — formaty są obowiązkowe
- [ ] zgłoszenie: `lablab.ai/.../simon/submission`
- [ ] **zgłosić dzień wcześniej.** Ostatnie godziny to moment, w którym platforma
      pada pod obciążeniem

### Czego świadomie NIE robimy w tym miesiącu

Automatycznego slashowania. Nie mamy progu, którego dalibyśmy radę obronić, a
pomiar ROCm może tę sprawę dopiero otworzyć. Wpisywanie go teraz byłoby
sprzedawaniem czegoś, czego sami nie umiemy uzasadnić — i pierwszy sędzia,
który zapyta „skąd ten próg", zobaczyłby to od razu.

Reszty checklisty M2.4V (commit/reveal, apelacja, koluzja, wykrywanie „zawsze
PASS", ukryte zadania kalibracyjne). To jest praca na kwartał, nie na miesiąc
z twardym terminem.

### Ryzyka, które realnie mogą to wywrócić

1. **Kredyt nie zostaje przyznany** → pomiaru nie ma. Plan awaryjny: Fireworks
   ($50) pokazuje SIMON działający na AMD, ale nie odpowiada na pytanie o dryf
2. **Szpon zasypia albo traci prąd w dniu oceny** → demo martwe. Dlatego drugi
   węzeł i `qwen-serving` w tydzień 1, a nie w ostatnim
3. **Tracki ogłoszone 12.10 nie pasują do SIMON-a** → zostaje opis przepisany
   pod trak; projektu nie naginamy

---

## KOREKTA FUNDAMENTALNA: M2 nie był momentem prawdy

**Co sprawdziłem w kodzie po uwadze Opusa:**

| Twierdzenie roadmapy | Faktyczny stan |
|---|---|
| „Computational Integrity ✅ ZROBIONE (D85 + TopLoc)" | ❌ **NIEPRAWDA** — to wpis z D109, którego kod nie realizuje |
| `activation_hash` jest weryfikowany | ❌ **NIKT go nie przelicza** — to zwykły string |
| Testy łapią fałszywy wynik | ❌ Łapią tylko **podmianę po podpisaniu** (`f3_receipt.rs:179`, `cykl_zlecenie.rs:237`) |

**Konsekwencja:** node, który **sam podpisze fałszywy hash i dowolny tekst**,
przechodzi całą weryfikację. Receipt dowodzi dziś, że **„zarejestrowany node X tak
twierdzi"** — nie, że policzył.

**M2 w starej wersji przeszedłby z node'em-echo.** To test łącza, nie tezy SIMON.

**Prawdziwy moment prawdy: Frank liczy model, Szpon weryfikuje, a oszust zostaje złapany.**

---

## Czego brakowało w roadmapie (i teraz jest)

1. **Weryfikacja obliczeń (M2.5)** — bez tego SIMON nie ma sensu
2. **D67** — D80 zrobiło z tego P0 „przed wszystkimi innymi". W roadmapie **nie było
   go nawet w „czego nie robimy"** — czyli dokładnie ten brak propagacji, przed którym
   ostrzega D77. **To był realny błąd, nie przeoczenie redakcyjne.**

---

## M0 — ZAMKNIĘCIE DZIUR (bez zamrażania formatu)

**M0 NIE zamraża formatu.** M1.1 dodaje nowe komunikaty, a weryfikacja obliczeń zmieni
receipt. Format da się zamrozić najwcześniej po M2.5.

| # | Zadanie | Dlaczego |
|---|---|---|
| ⏳ M0.2 | `order_id` = digest z pustym `order_id` + **losowy nonce** — **W TOKU** (kod + 3 testy; kompilacja workspace naprawiona 2026-09-17) | Licznik dawał ten sam `order_id` dla dwóch identycznych zleceń w różnych sesjach → stary `OrderCompleted` odtwarzalny |
| M0.1 | Podpisane `OrderAccepted` + **statyczna lista kluczy w pliku konfiguracyjnym** | Bez podpisu atakujący dobiera `coordinator_pubkey` dla konkretnego zlecenia (kilka hashy przy 1-2 uczciwych koordynatorach) |
| M0.3 | Weryfikacja Ed25519 w `BramkaPodpisu` | Dziś sprawdza tylko niepustość stringa — **teatr** (Ed25519 jest w `crypto.rs`) |
| M0.4 | **Pole `v: u16` w podpisanej treści** `Receipt` i `JobOrder` | Tematy mają `simon/v1`, ale **struktury nie mają wersji w podpisanej treści**. Później każda zmiana formatu = ręczne odróżnianie starych podpisów od nowych. Tanie teraz, drogie potem |

**M0.1 = ŚWIADOME UPROSZCZENIE:** „rejestr koordynatorów" na MVP to **statyczna lista
kluczy w pliku konfiguracyjnym**. Kto dopisuje klucze i kto jest źródłem zaufania —
to **decyzja strategiczna, świadomie odłożona** (nie rozrastamy M0.1 w projekt governance).

**Kryterium wyjścia (NIE liczba testów):** każda dziura ma test, który **padał przed
poprawką** i przechodzi po niej. 50 zielonych testów miało 4 dziury — liczba to złe kryterium.

## M1 — TRANSPORT DANYCH + BINARKA

| # | Zadanie | Uwaga |
|---|---|---|
| M1.1 | Kanał request-response dla promptu i wyniku | **⚠️ jawnie łamie RULES #1** — patrz niżej |
| M1.2 | Binarka `simon --role agent\|coord\|node --bootstrap <multiaddr> --listen /ip4/0.0.0.0/tcp/<port>` | Brak `fn main` = nie ma czego uruchomić |
| M1.3 | Czekanie na subskrypcję peera przed publikacją | Inaczej `InsufficientPeers` — w jednym procesie niewidoczne |

### ⚠️ M1.1 — JAWNA DECYZJA, NIE CICHA SPRZECZNOŚĆ

**Prompt wysłany do node'a kanałem request-response = node widzi TEKST.** To łamie RULES #1.

Obejście (liczenie pierwszych warstw u klienta) **nie przejdzie na telefonie
z dużym modelem**. Więc:

> **DECYZJA MVP: jeden node widzi prompt, szyfrowany w drodze.**
> Świadome odstępstwo od RULES #1, zapisane jawnie (D60: obrona nie może milczeć).

**Nie może to zostać cichą sprzecznością** — bo wtedy za pół roku ktoś przeczyta
RULES #1 i pomyśli, że działa, a nie działa.

## M2 — SZPON ↔ FRANK (prawdziwy vLLM)

| # | Zadanie | Uwaga |
|---|---|---|
| M2.1 | Statyczny bootstrap przez Tailscale | NAT i DHT na później |
| M2.2 | Weryfikacja zgodności `job_id` między maszynami | **Realny błąd** — `DefaultHasher` dawał różne `job_id` na różnych `rustc` |
| M2.3 | Frank uruchamia **prawdziwy vLLM**, nie echo | Inaczej test łącza, nie tezy |

## M2.5 — WERYFIKACJA OBLICZEŃ (prawdziwy moment prawdy)

| # | Zadanie | Kryterium |
|---|---|---|
| M2.5.1 | Weryfikator przelicza **TopLoc jednym prefillem** | — |
| M2.5.2 | **Test z oszukującym node'em PADA** | node podpisuje fałszywy wynik → złapany |
| M2.5.3 | **Pierwszy pomiar D67 na dwóch różnych kartach** | Szpon 3090 (24 GB) vs Frank 3080 Ti (12 GB) — **model MUSI być ≤14B**, inaczej nie zmieści się na Franku |

**To jest moment prawdy: Frank liczy, Szpon weryfikuje, oszust zostaje złapany.**

**D67 (jakość tokena) — P0 wg D80 „przed wszystkimi innymi".** Tu wraca do roadmapy
z jawnym miejscem, nie jako pominięty punkt.

## RT3 — MACIERZ ATAKÓW (zakres, nie milestone)

**Zasada skalowania stopniowego:** najpierw **po jednym ataku z każdej klasy**, potem 10-12 przypadków.

| Klasa atakującego | Cel | Gdzie testować |
|---|---|---|
| **Podsłuchujący node / farma destylacyjna** | prompty | symulacja (udział → wyciek) + kanarki na żywo |
| Oszust (fałszywe obliczenia, proxy do API) | integralność | node z oszustwem w M2.5 / M2.9 |
| Przejęcie koordynatora (`winner()`) | przydział, rozliczenie | **test w M0.1** |
| Powtórzenie zlecenia lub receiptu | rozliczenie | **test M0.2** |
| Sybil / wieloryb (wiele tożsamości, residential proxy) | kworum, podsłuch | symulacja D111, **wcześnie** |
| Kopanie z samym sobą / wash trading | emisja | symulacja ekonomii **po M2.5** |
| Eclipse (odcięcie peera w gossipsub) | dostępność, cenzura | test sieciowy po M2 |
| DoS / spam zleceń | dostępność | test po M2 |

### ⚠️ Podsłuch w MVP (M1.1) — szansa wycieku

**Założenie:** losowanie (VRF) rozdziela zlecenia proporcjonalnie do mocy → kto ma udział *s*,
widzi ~*s* wszystkich promptów. Model niezależności tur (szacunek, nie pomiar).

| Udział podsłuchującego | 1 tura | 10 tur | 50 tur | 200 tur |
|---|---|---|---|---|
| 1% | 1% | 10% | 39% | 87% |
| 5% | 5% | 40% | **92%** | 100% |
| 10% | 10% | 65% | 99% | 100% |
| 30% | 30% | 97% | 100% | 100% |

**Wystarczy jedna tura, bo harnessy agentowe wysyłają przy każdej turze CAŁY kontekst**
(pliki, historię, kod). Dzielenie sesji na wiele node'ów nic nie daje.

**Trzy wnioski (niewygodne):**
1. **Najwięcej GPU mają korporacje i państwa.** Laboratorium z 10% mocy zbiera prawdziwe
   zadania programistów — najcenniejsze dane treningowe. **A SIMON płaci mu za to w THINK.**
2. **Publiczna sieć MVP chroni kod GORZEJ niż komercyjne API** (API ma przynajmniej
   umowne zasady użycia danych, anonimowy node nie ma żadnych).
   **Hasło „nie karm korporacji" jest w tej wersji nieprawdziwe.**
3. Nie da się tego naprawić kryptografią na domowym sprzęcie: FHE ~3 s/token;
   poufne obliczenia GPU (TEE) są tylko na kartach serwerowych = sprzęt korporacji.

### Dwa różne ataki (nie mylić)

| Atak | Czy TopLoc łapie |
|---|---|
| Node udaje model X, przekazuje do Claude/GPT | **TAK** — hashe aktywacji nie zgodzą się z wagami (warunek: działający M2.5) |
| Farma zbiera prompty do destylacji | **NIE** — node liczy uczciwie, tylko zapisuje. To podsłuch |

### Co realnie działa (od najmocniejszego)

1. **Prywatne pody:** użytkownik wybiera „tylko node'y z mojej listy" (swoje, znajomych, firmy).
   **Jedyna prawdziwa ochrona na domowym sprzęcie, zgodna z D14. Dla kodu powinno być DOMYŚLNE.**
2. **Anonimizacja u klienta** przed wysłaniem (patrz M3.x) — ograniczenie: logikę kodu dalej widać.
3. **Kanarki na każdy node** — wyciek wskazuje sprawcę i uzasadnia utratę stawki.
   Działa tylko przy publicznym wycieku; państwowy aktor nie opublikuje.
4. **Jawna etykieta (D60):** „sieć publiczna = traktuj prompt jak publiczny".

### ❓ DECYZJA DLA KAROLA

**Czy SIMON publicznie obiecuje „twoje dane nie idą do korporacji"?**
Uczciwie da się to obiecać **tylko w prywatnych podach.**
W publicznej sieci MVP — **nie** (tabela wyżej).

**Zależność D111 ↔ wyciek:** limit mocy na operatora **jest też limitem wycieku**.
Jeśli symulacja sybili pokaże, że 10% udziału da się tanio kupić, **publicznej sieci dla kodu
nie da się obronić bez prywatnych podów.**

## M3.x — ANONIMIZACJA W `acp_server` (zależy od M3, nie blokuje M0)

**Gdzie:** w `acp_server`, **na maszynie użytkownika**. Harness sam dokleja wyniki narzędzi
(odczytane pliki, wyjście terminala) do każdej tury — **tylko agent SIMON widzi wszystko,
zanim cokolwiek wyjdzie do sieci.**

Kolejność: anonimizuj wejście → **bramka** → wyślij → **odwróć mapowanie w odpowiedzi**
(model odda kod z aliasami).

### ⚠️ Czego NIE wziąć wprost

| Narzędzie | Dlaczego nie wystarczy |
|---|---|
| `anonymize_for_ds.py` | dane osobowe z polskich dokumentów (PESEL, NIP, nazwiska). **Nie zna kluczy API, tokenów, hostów ani ścieżek** |
| Weryta Code (`code_ingest.py`) | granice z AST — **tylko Python**, a SIMON jest w Ruście. Zna definicje, nie wszystkie wystąpienia nazw |

### Co wziąć

1. **Wzorzec listy literałów** (`ANON_EXTRA.txt`): użytkownik wpisuje firmy, klientów, projekty,
   domeny → stabilne aliasy `[PROJEKT-1]`. Mapowanie zostaje lokalnie.
2. **Bramka po anonimizacji** (`case_study_privacy_gate.py`): wzorce strukturalne +
   prywatna lista; **blokuje wysłanie, jeśli coś zostało** (zasada RODO: trafienie = nic nie wychodzi).
3. **Weryta Code później, jako pomocnik** — tylko Python, tylko jeśli okaże się potrzebne.

### Kolejność według wartości

| # | Co | Dlaczego |
|---|---|---|
| 1 | **Sekrety i infrastruktura:** klucze, tokeny, hasła, IP, hosty, e-maile, ścieżki domowe | wyciek nieodwracalny, wykrycie tanie (wzorce gitleaks) |
| 2 | **Nazwy biznesowe z listy użytkownika** | klient, firma, produkt |
| 3 | **Zmiana nazw wszystkich identyfikatorów: RACZEJ NIE** | ocena niezmierzona: `f1(v3)` zamiast `oblicz_prowizje(klient)` → model odpowiada gorzej, a algorytm i tak widać. **Dużo psujesz, mało chronisz** |

**Pułapka przy odwracaniu:** częściowe dopasowania (`[PROJEKT-1]` sklejony z sąsiednim tekstem).
**Test na przejście w obie strony** (round-trip).

**Uczciwie:** anonimizacja ukrywa **kto i czyje**, a nie **co**. Farma destylacyjna dalej
dostaje realne zadania programistyczne — i właśnie to jest dla niej cenne.
**Przed zbieraniem danych chronią tylko prywatne pody.**

## M2.7 — ZAMROŻENIE FORMATU + RECENZJA

**Dopiero tutaj** format się zamraża (po M1.1, M2.5). Recenzja Opusa przed commitem.

## M2.9 — 10-12 PRZYPADKÓW (skalowanie stopniowe)

Różne długości promptów · node wypada w trakcie zlecenia · przekroczony timeout ·
oszust · **dwóch koordynatorów naraz**. Dopiero potem ACP.

## M5 — EKONOMIA: bezczynność zamieniona w moc szczytową (dopisane 2026-09-18)

**Model docelowy (Karol):** podłączasz maszynę, ktoś używa jej, gdy śpisz;
zbierasz kredyty; rano wydajesz je na zadania agentowe, ewentualnie szybciej
i na mocniejszym modelu; możesz odłożyć albo przekazać.

**Ta wizja jest sensowna, ale NIE jako „zarabiaj, gdy śpisz".** Rachunek:
3090 pod obciążeniem ~350 W × 8 h ≈ 2,8 kWh ≈ **3 zł**. Przy zmierzonych
65 tok/s i *stuprocentowym* obłożeniu to ~1,9 mln tokenów ≈ **2,2 zł** po
cenach rynkowych modeli otwartych. Czyli przy pełnym obciążeniu jesteśmy pod
progiem opłacalności, a obłożenia 100% nie będzie. **Ceną z komercyjnym API
nie wygramy i nie ma sensu próbować.**

Wartość leży gdzie indziej i tak to opisujemy:
1. praca, która NIE MOŻE wyjść na zewnątrz (nasza własna reguła RODO),
2. wynik, który da się sprawdzić, a nie tylko dostać,
3. **zamiana czasu bezczynnego na czas szczytowy** — w nocy jedna karta stoi,
   rano chcesz pięciu naraz. To jest realna zamiana, nie zarobek.

### Kamienie milowe

- [x] **M5.1 — podpisana DEKLARACJA ilości pracy.** Receipt niesie
      `prompt_tokens` i `completion_tokens`, ROZDZIELNIE. Jedna liczba
      „tokenów" byłaby zaproszeniem do arbitrażu: dekodowanie kosztuje ~55×
      więcej na token niż prefill (pomiar M2.4V).

      **KOREKTA 2026-09-18 (recenzja):** napisałem wcześniej „teraz mamy
      liczenie tokenów". To było przecenienie. Mamy **podpisaną deklarację
      węzła**, nie niezależnie potwierdzone zużycie. Podpis uniemożliwia zmianę
      liczby po fakcie; nie czyni jej prawdziwą. To samo dotyczy `ttft_ms`
      i `gen_ms` — niezależnie mierzymy wyłącznie czas obiegu po stronie
      klienta, więc **rozliczenia nie mogą opierać się na czasie zgłoszonym
      przez węzeł**.
- [ ] **M5.1b — liczniki przeliczane niezależnie.** Kontrakt docelowy:
      klient wysyła `input_token_ids` + `prompt_commitment` i sam liczy
      `declared_prompt_tokens = len(input_token_ids)`; węzeł zwraca
      `ordered_output_token_ids` + `decoded_text`; klient sprawdza
      `len(token_ids) == completion_tokens` oraz `decode(token_ids) == text`.
      Wtedy liczby nie da się zawyżyć bez dostarczenia pasującej sekwencji.
      Prompt musi być liczony tym samym tokenizerem co u węzła (repozytorium
      + rewizja, hashe plików, hash szablonu czatu, konfiguracja tokenów
      specjalnych, `add_generation_prompt`) — szablon czatu dokłada tokeny
      kontrolne, więc liczenie z samego widocznego tekstu daje inną wartość.
      BLOKER: żaden z naszych backendów nie wystawia dziś `token_ids` przez
      API OpenAI.
- [ ] **M5.1c — commitment po strukturze, nie po tekście.** Docelowo:
      `H("SIMON/OUTPUT/v1" ‖ job_id ‖ model_manifest ‖ tokenizer_manifest ‖
      prompt_commitment ‖ ordered_output_token_ids ‖ finish_reason ‖
      tool_calls ‖ attachments)`. Dziś hashujemy zdekodowany tekst — a różne
      sekwencje tokenów mogą dać ten sam tekst. Do interoperacyjności także
      kanonizacja RFC 8785 zamiast `serde_json::to_string`.
- [x] **M5.2 — lokalny, dopisywalny metrycznik podpisanej pracy.** ZROBIONE 2026-09-18.
      NIE portfel, NIE saldo. Rekord minimalny: hash receiptu, `job_id`, klucze
      klienta i wykonawcy, manifesty (model / tokenizer / profil wykonania),
      `prompt_commitment`, `output_commitment`, liczniki rozbite na
      `prompt_tokens_total` / `_computed` / `_cached` i `completion_tokens`,
      czasy ZMIERZONE PRZEZ KLIENTA, podpis oraz `verification_status`.

      `verification_status` musi rozróżniać `SIGNATURE_VALID`, `OUTPUT_BOUND`,
      `EXECUTION_AUDIT_PENDING|PASS|FAIL`. Jedno `verified: true` skleiłoby
      podpis, integralność treści i poprawność obliczenia w jedno mylące słowo —
      dokładnie ten błąd popełniłem w demo.

      Rejestr pokazuje SUROWE, rozdzielone liczniki i zmierzone czasy;
      **przelicznika na tym etapie NIE ustalamy.**

      Komunikat w demo brzmi „karta wykonała X jednostek pracy; wynik, liczniki
      i autorstwo są związane podpisanym receiptem, a poziom weryfikacji jest
      pokazany osobno" — **nie** „karta zarobiła X".
      **Stare rekordy (epoka 0) zostają nietknięte.** Podpisywał je klucz
      efemeryczny, więc dowodzą, że KONKRETNY klucz podpisał pracę, ale
      `ownership: UNRECOVERABLE` — nowa tożsamość nie może kryptograficznie
      udowodnić, że kontrolowała tamte klucze. **Nie wolno ich przepisać pod
      nowy klucz.** Rekordy niosą `identity_epoch`, a podsumowanie liczy je
      osobno.

      **Zrobione:** `simon_core::rejestr` (JSON Lines, tylko dopisywanie),
      `--rejestr <plik>` w agencie, podsumowanie w demo. Rekord ma pola
      z recenzji; te, których dziś nie mamy (manifesty modelu i tokenizera,
      `prompt_commitment`, rozbicie prefilla na policzony i z cache'u), są
      **NIEOBECNE, nie wyzerowane** — zaślepka wyglądałaby jak dane.
      `verification_status` rozróżnia pięć poziomów; dziś osiągalny jest
      `OutputBound`.

      **Efekt uboczny, który domyka lukę:** rejestr wykrywa powtórzenie
      receiptu (po `receipt_hash`, także po restarcie procesu). Whitepaper
      twierdził, że klient sprawdza jednorazowość receiptu — nie sprawdzał.
      Teraz jest gdzie to sprawdzić.
- [x] **M5.2a — trwała tożsamość klienta.** ZROBIONE 2026-09-18.

      **Dziura, którą to naprawia:** agent wołał `Keypair::generate()` przy
      KAŻDYM uruchomieniu, więc dwa uruchomienia były dwiema różnymi osobami,
      a `client_pubkey` w metryczniku był za każdym razem innym losowym
      kluczem. M5.2 ogłoszony jako zrobiony **nie potrafił przypisać pracy do
      nikogo**. Dowód z żywego pliku: 2 rekordy, 2 klucze.

      Zrobione: `simon_core::tozsamosc`, `--identity-file`, zapis atomowy
      (plik tymczasowy → fsync → 0600 → rename → ponowny odczyt → porównanie
      klucza publicznego), katalog danych użytkownika per system.
      **Nigdy nie regenerujemy klucza po błędzie** — uszkodzony plik, złe prawa
      albo brak możliwości zapisu zatrzymują start. Cicha regeneracja
      wyglądałaby jak udany start, a znowu rozcinałaby metrycznik.
      Sekret nie jest przyjmowany w wierszu poleceń ani w zmiennej
      środowiskowej (byłby w `ps`) i nie trafia do żadnego logu.

      Zweryfikowane na żywo: **2 zlecenia → 1 klucz klienta**, plik `0600`,
      zero trafień sekretu w wyjściu agenta.

- [ ] **M5.2b — SPECYFIKACJA wyprowadzania tożsamości.** Przed jakąkolwiek
      frazą odzyskiwania trzeba zamrozić format:
      definicja root-secret; rozstrzygnięcie, czy słowa kodują wprost
      32-bajtowe ziarno SIMON-a, czy używamy pełnego BIP-39 z jego PBKDF2
      i 64-bajtowym wynikiem jako materiałem dla HKDF (**tych wariantów nie
      wolno mieszać** — backup dawałby inne klucze w różnych implementacjach);
      etykiety HKDF z wersją (`SIMON/client-signing/ed25519/v1`,
      `SIMON/account-root/ed25519/v1`, `SIMON/node-signing/ed25519/<device>/v1`,
      `SIMON/libp2p/ed25519/<device>/v1`); wektory testowe między językami.

      **Osobny klucz per urządzenie, nie jeden dla wszystkich.** Wyprowadzenie
      tego samego klucza node'a na dwóch maszynach z jednej frazy przywróciłoby
      problem identycznych `peer_id`, naprawiony 2026-09-18. Zamiast tego:
      klucz konta certyfikuje, że dany node należy do konta.

      **Prywatność:** jeden globalny `client_pubkey` to globalny identyfikator
      korelacyjny — każdy operator node'a połączy całą historię klienta.
      Docelowo pseudonim per kontrahent albo per pod. Dziś status:
      `MVP_IDENTITY, privacy-preserving derivation: NOT DESIGNED`.

- [ ] **M5.2c — odzyskiwanie.** Eksport frazy dopiero po jawnym potwierdzeniu,
      przywracanie wyłącznie do pustego magazynu tożsamości, ochrona przed
      kolizją, weryfikacja kopii, rotacja i unieważnianie. **Nie nazywamy tego
      „odzyskiwaniem portfela", bo portfela nie ma.**

- [ ] **M5.3 — normalizacja jednostki.** Pierwsza jawna postać: `C = w_p·P + w_d·D`,
      gdzie `P` to **nie-cache'owane** tokeny promptu, `D` to tokeny wyjścia,
      a wagi są ZMIERZONE dla konkretnego profilu modelu. Manifest pracy musi
      trzymać więcej niż `C`: `prompt_tokens_total/_cached/_computed`,
      `completion_tokens`, hash manifestu modelu, profil wykonania,
      kwantyzację i przedział kontekstu.

      **Pułapka:** vLLM rozróżnia tokeny promptu, cache'owane i tworzące cache,
      bo to nie jest ta sama praca. Zaliczenie cache'owanego prefilla tak samo
      jak liczonego od zera otwiera kolejny arbitraż. Bez ogłoszonego kursu
      „tokeny" są walutą o kursie, którego nikt nie zna.
- [ ] **M5.4 — wydawanie.** Agent płaci kredytami, node sprawdza saldo.
      Tu zaczyna się problem: saldo musi być WSPÓLNE, a nie lokalne.
- [ ] **M5.5 — podwójne wydanie i konsensus.** Właściwy trudny kamień.
      Nie zaczynamy go przed rozstrzygnięciem M2.4V (slashing), bo to ten sam
      problem widziany z drugiej strony.
- [ ] **M5.6 — przekazywanie kredytów.** ŚWIADOMA decyzja, nie domyślna
      funkcja. Kredyt przenoszalny to już pieniądz: spekulacja, farmy Sybil,
      pytania regulacyjne. Nieprzenoszalny kredyt to system rozliczeń i tam
      zostajemy, dopóki nie będzie powodu.

### Atak, o którym trzeba pamiętać od pierwszego dnia

**Samoobsługa (wash-compute):** węzeł wysyła zlecenia sam do siebie i bije
kredyty z powietrza.

**KOREKTA 2026-09-18 (recenzja):** napisałem wcześniej, że obroną jest
weryfikacja. **To nieprawda.** Weryfikacja potwierdzi co najwyżej, że farma
NAPRAWDĘ wykonała pracę — dla samej siebie. Czyni to wash-compute uczciwym
obliczeniowo i nie usuwa arbitrażu ekonomicznego: przy `client == executor`
zapłata wraca do tego samego właściciela, a jego realny koszt to prąd plus
opłaty protokołu i weryfikatora. Jeśli przyznany kredyt ma większą użyteczność
niż ten koszt, samotransakcja pozostaje racjonalna.

Stąd twarde ograniczenia dla M5.2: rejestr **może** zapisywać wykonaną pracę,
ale **nie może** być saldem wymienialnych praw do przyszłej pracy ani emitować
nagrody za sztukę pracy. „Wykonane i zweryfikowane" nie znaczy „kupione przez
niezależny popyt". **Wash-compute pozostaje NIEROZWIĄZANY.**

### Zadania agentowe — już działają

Łańcuch agent→agent (B2) jest zrobiony: etap A liczy jeden węzeł/model,
zweryfikowany wynik idzie jako DANE do etapu B na innym węźle. To jest
dokładnie „rano daję zadania agentowi", tylko bez rozliczeń. Brakuje wyłącznie
rejestru (M5.2), żeby było widać, ile to kosztowało.

---

## M3 — WARSTWA ACP

| # | Zadanie | Kryterium |
|---|---|---|
| M3.1 | `agent-client-protocol-schema = "=1.7.0"` (przypięta) | cienki moduł tłumaczący |
| M3.2 | Transport stdio/WebSocket | `openclaw acp` obsługuje oba |
| M3.3 | `AgentThoughtChunk` na żywo, `AgentMessageChunk` po weryfikacji | rozstrzygnięcie konsylium |

**Kryterium (konkretne, nie „zewnętrzny harness się podłącza"):**
> **`openclaw acp` wysyła prompt i dostaje zweryfikowany wynik.**

## M4 — SYMULACJE

| # | Zadanie | Uwaga |
|---|---|---|
| M4.1 | **Symulacja sybil (D111)** | **Niezależna od M1-M3 → może iść RÓWNOLEGLE, i wcześniej** — decyduje o formacie rejestru |
| M4.2 | Symulacja D113 (degradacja zasobów) | Czy odcina patologię, czy przenosi na inny model? |

**M4.3 (federated learning) — WYCIĘTE.** Nikt tego nie potrzebuje do MVP.

---

## Kolejność (od Opusa)

**M0** (dziury, bez zamrażania) → **M1** (transport + binarka) → **M2** (Szpon↔Frank,
prawdziwy vLLM) → **M2.5** (TopLoc + oszust złapany + pomiar D67) →
**M2.7** (zamrożenie + recenzja) → **M2.9** (10-12 przypadków) → **M3** (ACP).

**M4.1 (sybil) równolegle, gdziekolwiek.**

**Kolejność wewnątrz M0: M0.2 → M0.1 → M0.3** (zatwierdzona).

---

## Czego świadomie NIE robimy

| Element | Powód |
|---|---|
| **Bielik-Guard** | D114 ODRZUCONY — miejsce w dokumentacji granic, nie w protokole |
| **Semantic Integrity (D109)** | OTWARTE — wymaga zmierzenia, czy rozbieżność jest wykrywalna |
| **„Kontekst przed modelem"** | Wycofane — test kontrolny pokazał obejście, nie naprawę |
| **Guardy per specjalizacja** | Najpierw pomiar, czy 0.1B nie mieści wiedzy |
| **Federated learning** | Nikt nie potrzebuje do MVP (Opus) |
| **Rejestr koordynatorów jako governance** | M0.1 = statyczny plik, świadome uproszczenie |

---

## Podział pracy

| Kto | Co |
|---|---|
| **Szpon** | Kod: M0 → M1 → M2 → M2.5. Testy, pomiary, dokumentacja |
| **Karol** | Decyzje strategiczne: kiedy Frank, klucze, kierunek |
| **Opus 5** | Recenzja kodu **przed zamrożeniem formatu**, red-team hipotez |

**Zasada z 2026-09-16:** Opus znalazł 4 dziury w kodzie z 50 zielonymi testami,
a potem **jeszcze 3** (podpisany `OrderAccepted`, `order_id` z licznika, brak
weryfikacji obliczeń). **Recenzja przed commitem formatu, nie po.**

---

## Horyzont

| Milestone | Bez blokerów | Z blokerami |
|---|---|---|
| M0 | 1 sesja | — |
| M1 | 2-3 sesje | request-response to nowa rzecz |
| M2 | 1 sesja | wymaga Franka online + vLLM |
| M2.5 | 2 sesje | **TopLoc w kodzie = nowa implementacja** |
| M2.7 | krótko | zależy od Opusa |
| M2.9 | 1 sesja | — |
| M3 | 1-2 sesje | zależność zewnętrzna |
| M4.1 | 1-2 sesje | równolegle |

**M2.5 to moment prawdy.** Wszystko przed nim to przygotowanie, wszystko po nim
to rozbudowa. **Dopóki oszust nie zostanie złapany — nie wiemy, czy SIMON działa.**

## Najbliższy krok

**M0.2** (`order_id` + nonce) → **M0.1** (podpisane `OrderAccepted`, statyczna lista
kluczy) → **M0.3** (Ed25519) → testy, które padały przed poprawką → commit.

_Last updated: 2026-09-16 (po recenzji Opusa 5)_

---

## SZACUNKI (ESTYMACJA) — nie pomiary

**Zasada:** przy każdej liczbie podane źródło. „Zmierzone" = pomiar z tej maszyny.
Reszta to szacunek i tak jest oznaczona. **Nie wolno tego czytać jako danych.**

### Wydajność — czego użytkownik realnie doczeka

| Składnik | Wartość | Skąd |
|---|---|---|
| TTFT | 1,74 s | **ZMIERZONE** (vLLM Szpon, prefix caching OFF) |
| Generowanie 27B na 3090 | 21,8 tok/s | **ZMIERZONE** (pilot, qwen3.8-27b) |
| Przez sieć | ~12 tok/s (21,8 × 0,55) | ⚠️ **ZAŁOŻENIE** — nie zmierzone |
| RTT | 20-80 ms, jednorazowo | szacunek (przy 1 node nie mnoży się) |
| Ogłoszenie + przyjęcie (gossipsub) | 0,1-1 s | heartbeat 1 s |
| Weryfikacja TopLoc | ~1-3 s (jeden prefill) | szacunek z paperu, **NIEZMIERZONE** |

**Odpowiedź na 500 tokenów: ~25-30 s** — i użytkownik **nic nie widzi przez cały czas**,
bo konsylium ustaliło: wynik po weryfikacji.

> **⚠️ NAJWIĘKSZA RZECZ DO PRZEMYŚLENIA.** „Weryfikuj przed pokazaniem" **zabija UX
> przy 22 tok/s**. API DeepSeeka oddaje to samo w 2-3 s **i strumieniuje**.
> Alternatywą było **optymistyczne zdanie gpt-oss** (strumieniuj od razu, sprawdzaj
> wyrywkowo po fakcie, oszusta karz utratą stawki) — konsylium je odrzuciło.
> **Po tych liczbach warto do tego wrócić.** To jest otwarte, nie rozstrzygnięte.

**Zaleta:** batching w vLLM = wiele strumieni naraz, przepustowość łączna wyższa
(niezmierzone).

### Baza modeli — pamięć karty wyznacza podaż

Q4 ≈ 0,6 GB / miliard parametrów + kontekst.

| Karta | Model realnie | Uwagi |
|---|---|---|
| 8 GB | 7-8B | proste zadania |
| 12 GB (**Frank**) | ≤14B | **qwen3.8-27b NIE zmieści się** |
| 24 GB (**Szpon**) | 27-32B dense / MoE ~30B (~3B aktywnych) | MoE dużo szybszy |
| 48-96 GB | 70B, większe MoE | mała część podaży |
| DeepSeek V4.1 (~475 GiB) | poza zasięgiem | wymaga lokalnych podów, nie WAN |

**Konsekwencja: M2.5 wymaga modelu ≤14B** (dopisane wyżej).

**Jakość:** w pierwszym roku SIMON sprzedaje otwarte modele do ~30B, **nie czołówkę**.
Akceptowalne dla kodu, streszczeń, przetwarzania wsadowego. **Nie konkuruje
z najlepszymi modelami agentowymi.** Różnica we wrześniu 2026 — **do zmierzenia
na własnych zadaniach, nie z benchmarków.**

### Użytkownicy — podaż łatwa, popyt trudny

**Wąskie gardło to POPYT.** Ludzie chętnie kopią (BOINC, Salad), ale dlaczego ktoś ma
**kupić** THINK, skoro API DeepSeeka ≈ $0,60/mln tokenów i odpowiada 10× szybciej?

**Realne źródła popytu:**
1. **Kopacze zużywają własne tokeny** — jedyny naturalny popyt od dnia 1
2. Odporność na cenzurę / brak jednego dostawcy
3. Prywatność — **ale decyzja MVP (jeden node widzi prompt) ją podważa.**
   **Nie da się tego sprzedawać jako „prywatne".**

| Etap | Aktywne node'y | Założenie |
|---|---|---|
| M2.9, M3 | 2-10 | własne maszyny + znajomi |
| Publiczna alfa (HN / r/LocalLLaMA po M3) | instalacji setki-niskie tysiące, **aktywnych po 30 dniach 50-300** | typowy spadek po premierze; churn 40% z BOINC **niezweryfikowany** dla płatnych kopaczy |
| Token na rynku | tysiące szybko, ale farmy i spekulanci | sybil (D111) + **prawo: MiCA (UE), KYC/AML (D12)** to bramki, nie przypisy |

### Ile kodu się zmieni

Dziś ~1200 linii Rusta w `src` + testy.

| Część | Rząd wielkości |
|---|---|
| M1: transport + binarka | 1-2 tys. linii |
| M2.5: node z vLLM + TopLoc | 2-4 tys. (**TopLoc jest w Pythonie** → Python obok Rusta) |
| M3: ACP | 0,5-1 tys. |
| **Księga i spalanie opłat** | **największa część, NIEOSZACOWANA** — `BurnProof` jest dziś wyłącznie syntetyczny |

> **Uczciwie: 50-80% dzisiejszego kodu zostanie przepisane przed produkcją.**
> To normalne i jest powód, żeby format zamrozić **dopiero w M2.7**.

### Precedens BTC/Monero — co niezmienne, co zmienne

| Projekt | Zmiany pod spodem |
|---|---|
| **Bitcoin** | 2010 błąd przepełnienia (~184 mld BTC), 2010 opcody, 2012 P2SH, 2013 przypadkowy split łańcucha, 2017 SegWit, 2021 Taproot |
| **Monero** | 2014 fork Bytecoina, 2017 RingCT, 2018 Bulletproofs (~80% mniejsze tx), 2018-19 zmiana algorytmu co pół roku, potem RandomX, 2020 CLSAG, 2022 pierścienie po 16, cichy fix błędu „key image" |

**Lekcja dla SIMON:**

| Warstwa | Status |
|---|---|
| **Niezmienne** | teza (THINK za zweryfikowaną pracę, spalona opłata), salda, wersjonowany format receiptu |
| **Zmienne bez wstydu** | transport, weryfikacja, placement, modele |
| **Mechanizm aktualizacji od dnia 1** | pole wersji (M0.4) + zasada, jak stary receipt żyje po zmianie |

**Oba projekty przetrwały krytyczne błędy, bo szybko naprawiały, a nie udawały,
że błędu nie ma.**

_Last updated: 2026-09-17_

---

## NARZĘDZIA (poza ścieżką M0–M3)

### Bramka statusów przez Weryta Code + parsowanie Rusta

**Problem, który rozwiązuje:** ta noc miała **cztery razy ten sam błąd** — status zapisany
szybciej, niż sprawdzony („5/7", „5/5", „ZROBIONE", „M0.2 ✅"). Za każdym razem złapała
to **ręczna recenzja**. Bramka może to zmechanizować.

```
ROADMAP / ARCH: „M0.2 ZAIMPLEMENTOWANE — test: m02_order_id.rs:NN"
bramka: czy plik:linia istnieje?                    → code_ask
        czy test odwołuje się do naprawianego symbolu?
        czy cargo test go uruchomił i przeszedł?
        brak dowodu → status odrzucony
```

**Klasa błędu D109** („Computational Integrity ZROBIONE", a `activation_hash` nikt nie
przelicza) odpadłaby na pytaniu: **„kto wywołuje weryfikację `activation_hash`?"** → nikt.

### Rozróżnienie, które łatwo pomylić

| Pytanie | Odpowiedź |
|---|---|
| W czym narzędzie jest napisane? (Python vs Rust) | **Dwie odpowiedzi, bo zależą od miejsca uruchomienia.** (a) Bramka statusów / narzędzie lokalne: przepisywać **NIE warto** — szybkość bez pomiaru, tygodnie pracy bez nowej funkcji. (b) **W `acp_server` u użytkownika (Mac, telefon): Rust jest argumentem DYSTRYBUCYJNYM** — jeden plik wykonywalny bez Pythona. Szybkość nie ma tam znaczenia |
| Jaki kod narzędzie rozumie? (dziś tylko Python przez `import ast`) | **Tu jest realna luka** — Weryta Code nie widzi kodu SIMON, bo SIMON jest w Ruście |

**Żeby czytać Rust, NIE trzeba przepisywać narzędzia.** Wystarczy nowy moduł wczytujący kod,
który wypluwa **ten sam format faktów** co `code_ingest.py`:
- Python + tree-sitter-rust, albo
- mały helper w Ruście na `syn`, zwracający JSON

**Szacunek: kilkaset linii (niezmierzone).** Resolver, `code_ask.py` i manifest zostają.

**ZMIERZONA WADA (2026-09-17):** zapytanie rośnie **liniowo z całym korpusem** — **271 ms
przy 400 wpisach**. To jest argument przeciw własnemu indeksowi w `acp_server`,
jeśli korpus miałby rosnąć.

### Dwa zastosowania (nie jedno)

| Zastosowanie | Po co | Warunek wejścia |
|---|---|---|
| **Bramka statusów** | łapie „ZROBIONE" bez dowodu | parsowanie Rusta, **jeden crate na start** |
| **Lokalny dobór kontekstu w `acp_server`** | mniej tokenów i mniej kodu w sieci przy każdej turze | eksperyment **tokeny na rozwiązane zadanie**, kryterium D70: **≥40% mniej tokenów wejścia przy nie gorszej trafności** |

### ⚠️ OGRANICZENIE (żeby nie powstało następne „ZROBIONE")

**Szansa podsłuchu z tabeli wycieku się NIE zmienia.** Spada tylko **ilość kodu w jednej turze**.

**9,4× zmierzono wobec „czytania pliku", a NIE wobec realnej sesji z historią rozmowy.**
To nie jest to samo porównanie — przy sesji z historią redukcja może być znacznie mniejsza.
**Nie przenosić tego mnożnika na deklarację o prywatności.**

### Kolejność (skalowanie stopniowe)

1. **NIE TERAZ** — M0.1 i M0.3 są ważniejsze
2. Parsowanie Rusta na **jednym** crate (`simon-core`, ~275 linii) + ręczne sprawdzenie faktów
3. Potem cały workspace + pierwsza bramka statusów na `ROADMAP.md`
4. Przepisanie całego narzędzia na Rust **tylko jeśli pomiar** pokaże, że Python jest za wolny

**Do anonimizacji (M3.x) parsowanie Rusta NIE jest potrzebne** — sekrety i lista literałów
to wyrażenia regularne, a zmianę nazw identyfikatorów i tak odradzono.

_Last updated: 2026-09-17_

---

## DECYZJA KAROLA — kontekst a cena (2026-09-17)

**Kolejność: C → D → B.**

| Etap | Co | Kiedy |
|---|---|---|
| **C** | **Obydwa tryby:** node deklaruje `max_ctx`, user może żądać — przecięcie brane, brak dopasowania = `OrderRejected` | TERAZ |
| **D** | Rekomendacja: twardy limit bez zmiany ceny (brak nowej ekonomii) | po C, przed alfą |
| **B** | Pełna taryfa zależna od kontekstu (`base + a×wejście + b×wyjście`) | przed publiczną alfą |

**Zmierzony stan limitów (2026-09-17, z `/v1/models`):**

| Node | Model | `max_model_len` |
|---|---|---|
| Szpon (3090) | qwen3.8-27b | 36 864 |
| Franko (3080 Ti) | bielik-awq | 8 192 |

Różnica 4,5× w puli — to samo zlecenie przechodzi na Szponie i wywala się na Franku.

**Ustalenie techniczne:** vLLM podaje `max_model_len` w `/v1/models`, więc node
**odczytuje limit przy starcie** zamiast go konfigurować ręcznie. Zero rozjazdu
konfiguracji z rzeczywistością.

**Dlaczego nie C jako cena:** ekonomia jawnie odłożona po M2.5 (`ROADMAP.md:110`).
Ale **limit `max_ctx` jest niezależny od ekonomii** — to higiena protokołu.
Bez niego za długie zlecenie kończy się błędem vLLM zamiast kontrolowanym
`OrderRejected`, a node dopłaca nie wiedząc, że dopłaca.

Analizy źródłowe: `docs/KV-CACHE-i-okno-kontekstowe-analiza.md`,
`docs/1M-kontekstu-co-sie-dzieje.md`, `docs/KONTEKST-a-cena-co-robic.md`.

---

## REGUŁA: podpisywana treść BEZ floatów (2026-09-17)

**Czwarty błąd tej samej rodziny** co `DefaultHasher` (M2.2): coś, co „wygląda
tak samo", ale nie jest tym samym po przejściu przez transport.

| Objaw | Przyczyna | Naprawa |
|---|---|---|
| `Err(BadSignature)` po stronie klienta, mimo poprawnego podpisu u node'a | `started_at`/`finished_at` jako `f64` — JSON i CBOR serializują floaty inaczej, więc `digest()` (SHA-256 z JSON-a) wychodził INNY po roundtripie | czas jako `u64` w **mikrosekundach** (`started_at_us`, `finished_at_us`) |

**REGUŁA:** w treści, która jest podpisywana i przechodzi przez transport
(gossipsub CBOR / request-response CBOR / JSON), **nie umieszczamy `f64`**.
Kolejność pól, kolejność kluczy i reprezentacja liczb muszą być deterministyczne
między formatami.

**Test regresyjny:** `crates/simon-core/tests/m26_cbor_roundtrip.rs` — sprawdza,
że `digest()` jest identyczny przed i po roundtripie. Padał przed poprawką.

### Rodzina błędów „format, który wygląda tak samo"

1. `DefaultHasher` — różne `job_id` na różnych `rustc` (M2.2)
2. `f64` w podpisywanej treści — różny digest po CBOR (2026-09-17)
3. Mock vLLM z `choices[].text` zamiast `choices[].message` — puste wyjście
4. `--model-hash` hardkod u agenta — uczciwy node odrzucany jako niespójny

**Wspólny wzorzec:** zakładamy, że dwie reprezentacje są równoważne, bo
wyglądają tak samo. Weryfikacja to sprawdza, ale dopiero FAKTYCZNIE uruchomiona
między dwiema maszynami.

---

## M3.x: chunkowanie + map-reduce dla dużych plików (2026-09-17)

Punkt 2 z `docs/KV-CACHE-spill-i-rozproszony-VRAM.md` sekcja 5 — plik większy
niż limit jednego node'a dzielony na kawałki (map), redukowany do jednej
odpowiedzi (reduce). Plan przeszedł przez `critique` PRZED kodem (gate 1,
`tmp/critique_plan_mapreduce.log`) — złapał realny blokujący błąd projektu:
jednostrzałowe zlecenie reduce na WSZYSTKIE wyniki cząstkowe przekroczyłoby
limit node'a dokładnie dla przypadku, dla którego ta warstwa miała istnieć
(1 MB pliku → ~72 kawałki → reduce ~21k tok. wejścia, 2,5× ponad limit
Bielika). Naprawa: redukcja **hierarchiczna** (`mapreduce::grupuj_do_budzetu`,
poziomami aż zostanie jeden wynik), nie jednostrzałowa.

**Dwa dodatkowe błędy złapane na ŻYWO (nie w testach jednostkowych) podczas
weryfikacji na prawdziwym node'zie (Szpon, qwen3.8-27b):**

1. **Budżet reduce nie może używać tego samego mnożnika co budżet map.**
   Map dzieli SUROWY plik nieznanego typu → konserwatywny worst-case
   (1,2 znak/tok, zmierzone na base64). Ale wejścia do reduce to ZAWSZE
   tekst wygenerowany przez model — proza (~3,5 znak/tok, sekcja 7 tego
   samego dokumentu). Użycie worst-case dla reduce dawało budżet za wąski,
   żeby zgrupować choćby 2 wyniki cząstkowe w drugiej rundzie — 20 kawałków
   zredukowało się do 6, a 6 nie chciało zejść dalej (`grupy.len() >=
   poziom.len()`, bramka poprawnie odmówiła zamiast zapętlić się w
   nieskończoność). Fix: `ZNAK_NA_TOKEN_PROZA_GENEROWANA` (3,5) dla budżetu
   reduce, worst-case zostaje TYLKO dla pierwszego podziału pliku.

2. **`libp2p-request-response` ma domyślny `request_timeout` = 10 sekund**
   (`Config::default()`, poziom PROTOKOŁU) — niezależny od tego, jak długo
   klient sam czeka (agent.rs miał własne 300s, które nic nie dawały). Przy
   TTFT >10s (zmierzone na żywo: 10,6-11,0s dla qwen3.8-27b pod obciążeniem)
   node liczył odpowiedź poprawnie, ale `send_response` padał, bo libp2p już
   ubił kanał po swoich 10s. Wyglądało jak zwis sieci — było ciche ustawienie
   fabryczne biblioteki, którego nikt świadomie nie wybrał. **To dotyczy
   KAŻDEGO użycia SIMON, nie tylko map-reduce** — jednostrzałowe zlecenia po
   prostu rzadko trafiały w >10s TTFT w dotychczasowych testach. Fix:
   `Config::default().with_request_timeout(Duration::from_secs(300))` w
   `simon-harness/src/request_response.rs`.

**Zweryfikowane end-to-end na żywo** (Szpon/qwen3.8-27b, dokument 11 921
znaków, 12 tematów, `--chunk-tokens 600 --map-max-tokens 50`): 20/20 kawałków
map, redukcja hierarchiczna 20→2→1 w 2 rundach, zero timeoutów, wszystkie
receipty zweryfikowane. Adaptacyjne dzielenie kawałka po `ctx_ponad_limit`
(node zwraca REALNĄ liczbę tokenów strukturalnie — `PromptError.realny_ctx`/
`limit_ctx`, nie w wolnym tekście `opis`, ta sama rodzina błędów co wyżej)
pokryte testami jednostkowymi, nie wymuszone na żywo w tym przebiegu.

**Świadomie NIE zrobione w v1** (nazwane, nie ukryte): rozkład kawałków na
WIELE node'ów naraz (to punkt 4, "rozproszony VRAM" — zależny od tej
warstwy); pełny `--resume` (jest tylko checkpoint na dysk jako siatka
bezpieczeństwa, wznowienie ręczne); pełna obrona przed prompt injection
międzykawałkowym (jest mitygacja przez ramowanie "to DANE, nie instrukcje",
nie formalne rozwiązanie).

Kod: `crates/simon-cli/src/mapreduce.rs` (czysta logika, testowalna bez
sieci), `crates/simon-cli/src/agent.rs` (orkiestracja), `--file`/
`--chunk-tokens`/`--map-max-tokens` w CLI.

---

## Poboczne: LiteLLM + 4 modele na Franko (2026-09-17) — NIE część SIMON

Po zamknięciu punktu 4 padła propozycja "LiteLLM + 4 modele na Franko" —
świadomie nazwana jako **nowy, osobny temat** (routing wielu modeli na
JEDNYM node, nie rozproszony VRAM jednego modelu). Nie dotyka kodu SIMON,
zapisane tu tylko jako kontekst decyzji (Franko było ostatni dzień wolne
przed zajęciem GPU).

**Stan (uczciwie, "zostaw jak jest"):** 2 z 4 modeli GGUF (z LM Studio na
Franko) działają współbieżnie przez LiteLLM (`http://<node-windows>:4000`,
OpenAI-compatible, routing po nazwie modelu):
- `r1-qwen-7b` (DeepSeek-R1-Distill-Qwen-7B) ✓ ~112 tok/s
- `r1-llama-8b` (DeepSeek-R1-Distill-Llama-8B) ✓ ~125 tok/s
- `r1-qwen-14b`, `gpt-oss-20b` — skonfigurowane w LiteLLM, NIE wystartowane
  (każdy ~11 GB, karta 12 GB nie pomieści żadnego z nich razem z dwoma
  małymi już działającymi — VRAM finalne: 11 190/12 288 MiB)

**Dwa realne problemy infrastrukturalne złapane i naprawione po drodze**
(przydatne poza tym zadaniem, Franko = Windows + WSL2 Ubuntu-24.04):
1. `nohup`/`setsid` NIE chroniły procesu przed śmiercią po zamknięciu sesji
   SSH→WSL (`wsl.exe -d Ubuntu-24.04 -- bash script.sh` ubija drzewo
   procesów gdy front-end `wsl.exe` się kończy, mimo detach po stronie
   Linuksa). Fix: `Invoke-CimMethod -ClassName Win32_Process -MethodName
   Create` z PowerShell — genuinie niezależny proces Windows, przeżywa
   rozłączenie SSH. Ten sam trik już wcześniej rozwiązał identyczny problem
   z serwerem TTS TARS na tej samej maszynie.
2. Nowe porty (8101/8102/4000) nie miały reguł `netsh interface portproxy`
   Windows→WSL (WSL2 NAT nie przepuszcza ruchu z zewnątrz bez explicit
   forward) — dodane analogicznie do istniejącej reguły dla portu 8000
   (vLLM bielik-awq).

**Znane, nienaprawiane (na życzenie — "zostaw jak jest"):** LiteLLM z
`general_settings.master_key` włącza ścieżkę auth, która przy pewnych
żądaniach próbuje dociągnąć moduł `prisma` (baza) i pada z 500 — w moich
testach `/v1/models` i `/health/readiness` z nagłówkiem `Authorization`
działały poprawnie, ale zgłoszono inny wynik na innej ścieżce żądania.
Niezweryfikowane dalej, config i procesy pozostają bez zmian.

---

## B2: łańcuch agent→agent (CODER→TESTER, przekazanie bez ręcznego przeklejania)

**Plan przeszedł przez `critique` przed kodem** (`tmp/critique_plan_b2.log`).
Pierwotna propozycja: nowy protokół P2P `/simon/agent/1`, agent jako serwer
nasłuchujący, struktura `AgentHandoff` z licznikiem `hop`/`max_hops`. **Zanim
to zbudowałem, sprawdziłem czy w ogóle jest potrzebne** (ladder YAGNI z
ponytaila: "czy to musi istnieć jako nowa warstwa?") — nie musi. "Agent do
agenta" (case B: task split, w odróżnieniu od case A: prompt split =
map-reduce) to w praktyce: weź zweryfikowany wynik etapu A, zbuduj z niego
prompt etapu B, wyślij do INNEGO node'a. Zero nowego protokołu, zero nowego
trybu nasłuchu, zero hop-limitu (bo nie ma pętli — to liniowy łańcuch A→B,
długość ustalona przez wołającego, nie przez protokół).

**Zaimplementowane (Opcja 2 z planu):** nowe flagi na `--role agent`:
`--then-bootstrap`/`--then-model-hash`/`--then-prompt`. Reużywa w 100%
istniejące `zbuduj_zlecenie`/`wyslij_i_czekaj`/`zweryfikuj_receipt` — jeśli
etap A pada (błąd node'a LUB receipt nie przechodzi weryfikacji), łańcuch
przerywa się przez `?` PRZED wywołaniem etapu B (gwarancja kontroli przepływu
Rust, nie dodatkowa logika do przetestowania). Wynik etapu A trafia do
promptu etapu B ramowany jak w map-reduce (`===WYNIK ETAPU A START/KONIEC===`,
"to DANE, nie instrukcje") — **mitygacja, NIE formalna gwarancja**, ten sam
uczciwie nazwany limit co w map-reduce (tekst wewnątrz danych mógłby podszyć
się pod ogranicznik).

**Poprawki po krytyce (DS, gate 1):** walidacja `--then-bootstrap` wymaga
`--then-prompt` PRZED jakąkolwiek robotą sieciową (nie po kosztownym etapie
A), smoke test na mocku (reprodukowalny, `tmp/b2_chain_smoke.sh` — bez
zależności od żywego GPU), testy jednostkowe na ramowanie anty-injection i
parsowanie peera.

**Zweryfikowane:**
- Mock (reprodukowalne): `tmp/b2_chain_smoke.sh`, RC=0, oba etapy
  zweryfikowane.
- Żywe (Szpon, qwen3.8-27b, ten sam node jako "CODER" i "TESTER" — dowodzi
  mechanizmu łańcucha, nie routingu cross-model, to już było dowiedzione
  wcześniej w teście 2 równoczesnych agentów): `tmp/b2/chain_live.log`,
  RC=0, oba receipty spójne.

Pełny test suite: 23 testy w `agent.rs` (4 nowe B2), zero regresji.

---

## Security gates accepted after SIMON 1.0 review (2026-09-17)

**Źródło:** audyt zewnętrzny `docs/refs/SIMON-1.0-audyt-2026-09-17.md` (status: EXTERNAL-REVIEW).
Poniżej formalnie przyjęte bramki. Higiena produkcyjna jest w osobnej sekcji niżej —
nie konkuruje wizualnie z blockerami tezy.

### Protocol-blocking P0

1. **Proof soundness**
   TopLoc jest traktowany jako **probabilistyczny audyt wykonania**, nie kompletny
   kryptograficzny dowód inferencji.
2. **Independent verification**
   Protokół wymaga roli **VERIFIER**. Jej przydział, wynagrodzenie, mechanizm
   antykoluzji i obowiązki weryfikacyjne są **NIEZAPROJEKTOWANE**.
3. **Settlement finality**
   `BurnProof`, ochrona przed replay, finalność escrow i obsługa double-spend muszą
   istnieć, zanim zostaną włączone tokeny o wartości ekonomicznej.
4. **Wash-compute**
   Opłacona i zweryfikowana praca **nie dowodzi zewnętrznego popytu**. Emisja
   per-work pozostaje **ZABLOKOWANA**, dopóki samotransakcja nie może dać
   dodatniej wartości oczekiwanej.

### Product-blocking P0

5. **Provisional output authority**
   Niezweryfikowany wyjściowy strumień **nie może** wykonywać uprzywilejowanych
   tool calli ani trwale zmieniać stanu klienta.

### Production security requirements (higiena — NIE blokery tezy)

Sandbox bez egressu, SBOM i reproducible image, pinning zależności i CVE gate,
allowlista manifestów modeli (hashe, `safetensors`, brak remote code), rate limiting
przed kosztownym tokenizowaniem, limity CPU/RAM/dysku/timeoutu, walidacja długości
po prawdziwym tokenizerze, ochrona przed decompression bombs i Unicode worst-case,
rejestr licencji modeli, DPIA i opis ról GDPR przed publicznym uruchomieniem.

---

## M2.4V — VERIFIER DESIGN (przed M2.5, nowy etap)

**Warunek wejścia w M2.5.** Bez tego SIMON zastąpiłby nieudowodnionego executora
nieudowodnionym verifierem.

- [x] koszt jednego audytu **ZMIERZONY** — 2,4-9,9% zlecenia (mediana 7,4%),
      Szpon/qwen3.8-27b; `tmp/m24v_koszt_audytu.json`, projekt: `docs/design/DESIGN-VERIFIER-0.md`.
      Zastrzeżenie: mierzy przebieg w przód (dolne oszacowanie), bez ekstrakcji aktywacji
      i bez narzutu commitmentu po stronie wykonawcy
- [ ] źródło wynagrodzenia wskazane (z opłaty za job, NIE z dodatkowej emisji)
- [ ] commit/reveal określony (verifier losowany po commicie executora; nie widzi
      odpowiedzi innych przed własnym commitem)
- [~] zasady slasha i appeal — **częściowo, oparte na pomiarze**: dryf uczciwy
      zmierzony (jedna karta, najlepszy przypadek): 51% pozycji NIE jest
      bit-identycznych, 12% ma Δ>1e-3, a dryf **koncentruje się w pozycjach
      niskiego marginesu **logitów wyjściowych** (1,25 vs 9,125).
      ZAMKNIĘTE: bitowe porównanie wartości numerycznych wykluczone jako podstawa slasha.
      ZASADA: PASS → rozliczenie; SOFT FAIL (dryf niejednoznaczny) → eskalacja BEZ slasha;
      HARD FAIL (dowód niezależny od arytmetyki) → slash. `tmp/m24v_dryf_rozklad.json`.
      HIPOTEZA, NIE wynik: że to ten sam obszar, w którym chowa się atakujący —
      zmierzono margines LOGITÓW, a TopLoc używa marginesu granicy top-k AKTYWACJI.
      NIEWYKAZANE: że żaden próg nie oddziela — zmierzono tylko H_0, brak H_1.
      n=400 to za mało na ogon: p99 jest sygnałem, nie parametrem protokołu.
      D1 ZROBIONY u Franka (3080 Ti sm_86): aktywacje top-128 na ścieżce prefill
      były **bit-identyczne w 100%**, zero zmian indeksów (0/409472) — odwrotnie
      niż logprobs. ALE pomiar OBCIĄŻONY: zmieniono 3 zmienne naraz (model,
      kwantyzacja, stos serwujący). HIPOTEZA: dryf pochodzi ze STOSU SERWUJĄCEGO
      (vLLM, dynamiczny batching), nie z karty — co, jeśli prawda, pozwala narzucić
      kanoniczną ścieżkę weryfikacji i odzyskać powtarzalność. `tmp/m24v_d1_dryf_aktywacji.json`.
      OTWARTE: D2 (ten sam model dwiema ścieżkami — izolacja zmiennej), macierz A-G,
      model produkcyjny/AWQ, decode, H_1 (ataki)
- [ ] koluzja client–executor–verifier opisana
- [ ] test „zawsze PASS" musi zostać wykryty
- [ ] okresowe ukryte zadania kalibracyjne

**Status ekonomii verifiera: HIPOTEZA.** Odporność na koluzję i Sybil wymaga
symulacji oraz adversarialnego testnetu. Verifier nie może zarabiać wyłącznie za
złapanie oszusta — przy uczciwej sieci traci przychód i racjonalnie przestaje
sprawdzać (*verifier's dilemma*).
