---
status: DESIGN
code: NONE
applies_to: SIMON — blocker P0 „independent verification" (ROADMAP, M2.4V)
data: 2026-09-17
---

# DESIGN-VERIFIER-0 — rola weryfikatora

**Ten dokument nie opisuje zaimplementowanego mechanizmu.** Odpowiada na sześć
pytań, bez których budowanie verifiera oznaczałoby zastąpienie nieudowodnionego
executora nieudowodnionym verifierem. Każda odpowiedź ma jawny status:
**ZMIERZONE**, **DECYZJA**, **HIPOTEZA** albo **OTWARTE**.

---

## 1. Co dokładnie verifier przelicza?

**DECYZJA (wynika z fizyki, nie z preferencji):** verifier **nie generuje
odpowiedzi ponownie**. Dostaje pełną, już znaną sekwencję (prompt + wygenerowane
tokeny) i wykonuje **jeden przebieg w przód** po całości — czyli *teacher
forcing*, nie dekodowanie. Z tego przebiegu wyciąga aktywacje we wskazanych
punktach, liczy fingerprint i porównuje z commitmentem wykonawcy.

To rozróżnienie jest sednem całej ekonomii roli: **ponowne wygenerowanie
kosztowałoby tyle co oryginalne zlecenie** (i rola byłaby martwa), a przebieg
w przód po znanej sekwencji kosztuje kilka procent — patrz §2.

Do audytu verifier musi mieć dokładnie to samo, co miał wykonawca:

| Składnik | Dlaczego |
|---|---|
| wagi modelu (ta sama rewizja i kwantyzacja) | inna kwantyzacja = inne aktywacje |
| tokenizer + szablon czatu | inny podział na tokeny = inna sekwencja |
| pełna sekwencja (prompt + output) | bez outputu nie ma czego sprawdzać |
| parametry próbkowania i seed | do odtworzenia warunków |
| commitment wykonawcy (do czego porównujemy) | inaczej audyt nie ma odniesienia |

**Konsekwencja, którą trzeba nazwać:** verifier **widzi prompt i output w
całości**. Rola weryfikatora **powiększa liczbę stron widzących treść** —
z jednej (node) do dwóch lub więcej. To bezpośrednio pogarsza sytuację z §6
whitepapera (podsłuch) i musi być policzone w tabeli szansy wycieku, zanim
ktokolwiek ogłosi liczbę weryfikatorów na zlecenie.

**Druga konsekwencja (złapana w krytyce): tokeny rozumowania.** Model, który
generuje ukryte rozumowanie (jak qwen3.8 w naszych pomiarach), wygenerował je
**jako tokeny** — więc verifier musi je dostać, żeby odtworzyć sekwencję. Albo
rozumowanie idzie do weryfikatora (i prywatność jest gorsza, niż sądzi
użytkownik, który widzi tylko odpowiedź), albo audyt obejmuje niepełną
sekwencję. **Nie da się mieć obu naraz** — do rozstrzygnięcia przed wyborem
modeli rozumujących do sieci.

### Twardy limit: zlecenie blisko okna kontekstu jest NIEAUDYTOWALNE jednym przebiegiem

Audyt wymaga, żeby **cała sekwencja `prompt + output` zmieściła się w oknie
modelu naraz**. Zlecenie, które samo zjada większość okna, nie ma jak zostać
sprawdzone w jednym przebiegu: przy `max_ctx = 8192` (Bielik na Franku) i
zleceniu `prompt 7000 + output 500` sekwencja audytowa ma 7500 tokenów — jeszcze
wchodzi, ale `prompt 7800` już nie. **Dzielenie na kawałki łamie teacher
forcing** (kawałek nie zna stanu z poprzedniego), więc nie jest darmowym
obejściem.

**Konsekwencja dla bramki C (`ctx_ponad_limit`, zaimplementowana 2026-09-17):**
limit dla zleceń, które mają być audytowalne, jest **niższy** niż `max_model_len`
node'a — musi zostawić miejsce na wygenerowane tokeny. To jest nowy wymóg wobec
istniejącej bramki, nie hipoteza.

---

## 2. Ile kosztuje jeden audyt względem zlecenia? — ZMIERZONE

Pomiar na Szponie (RTX 3090, qwen3.8-27b przez vLLM, 2026-09-17). Metodyka jak
w pozostałych pomiarach tego dnia: unikalna losowa treść per punkt (bez
aliasingu w prefix cache), warm-up odrzucony, `/tokenize` do dostrojenia.

| prompt (tok.) | output (tok.) | sekwencja | zlecenie | audyt | **audyt = % zlecenia** |
|---|---|---|---|---|---|
| 500 | 50 | 551 | 7,9 s | 0,63 s | **8,0%** |
| 503 | 200 | 704 | 20,0 s | 0,73 s | **3,7%** |
| 2016 | 200 | 2217 | 18,8 s | 1,86 s | **9,9%** |
| 2011 | 500 | 2512 | 87,1 s | 2,07 s | **2,4%** |
| 6037 | 500 | 6538 | 69,1 s | 5,12 s | **7,4%** |

**Wynik: 2,4–9,9% zlecenia, mediana 7,4%.** Przepustowość przebiegu
audytowego: 873–1276 tok/s (rośnie z długością — stały narzut się amortyzuje).

**Dlaczego tak tanio — i kiedy przestanie być tanio:**

```
audyt    ≈ (N+M) / R_prefill
zlecenie ≈ N / R_prefill  +  M / R_decode
```
Przy zmierzonych `R_prefill ≈ 1200 tok/s` i `R_decode ≈ 22 tok/s` dekodowanie
jest ~55× wolniejsze na token. Stąd dla długich odpowiedzi audyt jest tani, a
**dla krótkich odpowiedzi przy długim promptcie drożeje** — granicznie dąży do
`(N+M)/M · 1/55`. Zlecenie „wielki prompt, jedno zdanie odpowiedzi" jest
najgorszym przypadkiem dla tej ekonomii.

**Zastrzeżenia (bez nich liczba kłamie):**
- Mierzy **dominujący składnik** — przebieg w przód. Prawdziwy TopLoc dokłada
  ekstrakcję aktywacji, hash i porównanie; API OpenAI vLLM tego nie wystawia.
  **To jest dolne oszacowanie.**
- Jedna karta, jeden model, jedna kwantyzacja. Macierz sprzętowa nie zrobiona.
- Wariancja czasu zlecenia jest duża (87 s vs 69 s przy dłuższej sekwencji) —
  zależy od tego, ile tokenów rozumowania model faktycznie wygeneruje.
- Pierwszy przebieg pomiaru **był błędny i został odrzucony**: skrypt czytał
  pole `reasoning_content`, a vLLM zwraca `reasoning`, więc do audytu doklejał
  jeden znak zamiast całego wyjścia. Błąd złapany po tym, że `seq ≈ prompt`.
  Do skryptu dopisano bramkę, która teraz przerywa pomiar w takim przypadku.
  Surowe dane: `tmp/m24v_koszt_audytu.json`.

**Wniosek dla ekonomii:** przy ~5-10% kosztu audytu **da się zapłacić
weryfikatorowi z opłaty za zlecenie, bez podwajania ceny dla użytkownika.**
To jest warunek konieczny istnienia roli — i jest spełniony.

---

## 3. Kto płaci?

**DECYZJA:** z **opłaty za zlecenie**, nigdy z dodatkowej emisji. Emisja za
pracę jest zablokowana do czasu rozwiązania wash-compute (blocker P0 nr 4) —
finansowanie weryfikacji emisją odtworzyłoby dokładnie ten problem.

```
OPŁATA ZA ZLECENIE
├── wynagrodzenie wykonawcy
├── rezerwa weryfikacyjna      ← stąd płaci się verifierowi
└── opłata protokołu/rozliczenia
```

**DECYZJA:** verifier dostaje zapłatę **za terminowe wykonanie przydzielonego
audytu, niezależnie od werdyktu PASS/FAIL**. Płacenie wyłącznie za złapanie
oszusta to *verifier's dilemma*: w uczciwej sieci weryfikator nie ma przychodu
i racjonalnie przestaje liczyć, odpowiadając `PASS` w ciemno.

**HIPOTEZA:** dodatkowy udział w zabranej stawce — tylko za **udowodnione**
oszustwo, nie za sam werdykt FAIL. Wysokość nieustalona; zbyt duży udział tworzy
zachętę do fałszywych oskarżeń.

**OTWARTE:** czy rezerwa weryfikacyjna jest pobierana od **każdego** zlecenia
(wtedy audyt wyrywkowy oznacza, że większość rezerw finansuje mniejszość
audytów), czy tylko od losowanych. Pierwsze jest prostsze i wygładza koszt,
drugie jest uczciwsze cenowo. Nie rozstrzygam bez danych o odsetku audytów.

---

## 4. Jak verifier jest losowany?

**DECYZJA (kolejność, nie mechanizm):** **najpierw commitment wykonawcy, potem
losowanie.** Wykonawca publikuje zobowiązanie do śladów **zanim** dowie się,
kto go sprawdzi i które punkty zostaną skontrolowane. Odwrotna kolejność
pozwala dobrać, co pokazać.

**Koszt tej kolejności (złapany w krytyce, nie policzony w §2):** skoro
wykonawca nie wie, które punkty zostaną sprawdzone, **musi zobowiązać się do
wszystkich** — czyli policzyć i zahashować aktywacje dla całej sekwencji, nie
tylko dla wylosowanych pozycji. Pomiar z §2 obejmuje **tylko stronę
weryfikatora**; narzut po stronie wykonawcy (czas + rozmiar commitmentu, np.
korzeń Merkle) jest **niezmierzony**. Publikowane w TopLoc ~8 B/token i
~0,26 ms narzutu sugerują, że jest mały, ale **to jest liczba z paperu, nie
z naszego sprzętu** — do zmierzenia razem z implementacją.

**DECYZJA:** verifier losowany z **innej puli/domeny** niż wykonawca — ten sam
podmiot nie może być jednocześnie liczącym i sprawdzającym tego samego zlecenia.

**OTWARTE — źródło losowości.** VRF, publiczny beacon i „hash commitmentu"
mają różne właściwości wobec **grindingu** (zmiana wejścia aż do korzystnego
wyniku). Nie wpisuję żadnego, bo bez rozstrzygnięcia, kto generuje seed i kiedy
wejście jest zamrożone, wybór byłby deklaracją bez pokrycia.

**OTWARTE — liczba weryfikatorów.** Świadomie **nie** wpisuję „komitet 3-7"
ani kworum. Liczba weryfikatorów wprost mnoży koszt (§2) **i liczbę stron
widzących prompt** (§1), więc jest decyzją ekonomiczno-prywatnościową, nie
techniczną. Wymaga danych z symulacji.

---

## 5. Jak wykrywamy verifiera, który zawsze odpowiada PASS?

**HIPOTEZA:** okresowe **ukryte zadania kalibracyjne** — zlecenia o znanym z
góry wyniku, w tym celowo zepsute, wstrzykiwane w normalny ruch. Verifier, który
przepuszcza spreparowany błąd, sam się demaskuje.

**OTWARTE i trudniejsze, niż wygląda:**
- **Nierozróżnialność.** Jeśli zadanie kalibracyjne da się odróżnić od
  prawdziwego (po rozmiarze, źródle, czasie, kliencie), leniwy verifier
  sprawdza tylko je. To jest wyścig, nie rozwiązanie.
- **Kto je generuje?** W sieci bez uprawnień nie ma naturalnego kandydata.
  Generator zna prawidłowe odpowiedzi, więc jest pozycją zaufaną — czyli
  wprowadza dokładnie to, czego SIMON unika.
- **Koszt.** Zadania kalibracyjne to praca, za którą ktoś płaci, a która nie
  daje użytkownikowi wartości.

**Nie twierdzę, że to jest rozwiązane.** Dopóki nie jest, „weryfikator liczy
naprawdę" ma taki sam status, jaki miało „node liczy naprawdę" przed tym
audytem.

---

## 6. Co przy rozbieżnych werdyktach?

**DECYZJA (zasada, nie parametr):** rozbieżność **eskaluje**, nie jest
rozstrzygana większością na tym samym poziomie. Pojedynczy uczciwy verifier musi
móc **zatrzymać** błędne przyjęcie — reguła większości przy skolidowanej
większości daje dokładnie odwrotny efekt.

**Cena tej zasady (złapana w krytyce):** jeśli pojedynczy sprzeciw zawsze
eskaluje, to **pojedynczy złośliwy verifier może zablokować każde zlecenie**,
sprzeciwiając się zawsze. Zamieniamy „skolidowana większość przepycha fałsz" na
„jeden wredny blokuje sieć". Obie skrajności są nie do przyjęcia, więc potrzebny
jest **limit czasu i koszt sprzeciwu** — eskalacja nie może być darmowa ani
nieograniczona. **OTWARTE:** ile kosztuje sprzeciw i co się dzieje, gdy
eskalacja nie kończy się w terminie.

**DECYZJA:** **slash tylko za dowód jednoznaczny.** Różnice numeryczne
(inny sterownik, inna karta, inny silnik, batching, prefix cache) **nie są
dowodem oszustwa**. Bez zmierzonego rozkładu „uczciwego dryfu" dla danej
kombinacji model–kwantyzacja–GPU–sterownik status brzmi `NIEWERYFIKOWALNE`,
a nie automatyczny PASS ani automatyczny slash.

**Pułapka tej decyzji (złapana w krytyce):** jeśli „jednoznaczny" ustawimy zbyt
wysoko, a dryf numeryczny jest wszechobecny, to **nikt nigdy nie zostanie
ukarany** — i mamy weryfikację, która nic nie egzekwuje. To jest ta sama klasa
błędu co „bramka, która zawsze przepuszcza". Jednoznaczne pozostają rzeczy
**niezależne od arytmetyki**: podpisanie dwóch sprzecznych commitmentów,
niezgodny `model_root`, replay, brak odpowiedzi. Różnice aktywacji **same
w sobie** nadają się na kwarantannę i eskalację, nie na zabranie stawki —
dopóki nie ma zmierzonego rozkładu dryfu, który wyznaczy próg.

**Zasada wynikająca wprost z pomiaru dryfu:** przekroczenie probabilistycznego
progu **nie może bezpośrednio powodować slasha**. Przy ciężkoogonowym rozkładzie
uczciwym pojedynczy próg prędzej czy później ukarze poprawny node. Trzy wyjścia,
nie dwa:

```
PASS
  → rozliczenie

SOFT FAIL / dryf niejednoznaczny
  → drugi verifier albo dokładniejsze powtórzenie
  → BEZ automatycznego slasha

HARD FAIL / obiektywny dowód oszustwa
  → slash
```

Do `HARD FAIL` nadają się wyłącznie zdarzenia **niezależne od arytmetyki**:
sprzeczne podpisane commitmenty, niezgodny hash wag (`model_root`), replay
receiptu albo niezgodność utrzymująca się przez całą procedurę eskalacyjną.

**Ryzyko techniczne do zmierzenia przed progiem:** teacher forcing po całej
sekwencji **nie musi dać tych samych aktywacji** co inkrementalne dekodowanie,
jeśli model używa okna przesuwnego w uwadze albo kwantyzacji KV cache
(u nas: `--kv-cache-dtype bfloat16` na Szponie, `q4_0` w innych konfiguracjach).
Taka rozbieżność dałaby **fałszywe FAIL na uczciwych node'ach** — i jest to
pomiar do zrobienia przed jakąkolwiek implementacją slasha.

**OTWARTE:** ścieżka eskalacji (większy komitet / pełne powtórzenie spornego
segmentu / tryb TEE jako klasa premium), procedura odwoławcza i kto płaci za
eskalację.

---

## Czego ten dokument NIE rozwiązuje

| Problem | Status |
|---|---|
| Koluzja klient–wykonawca–verifier (jeden operator, trzy role) | **OTWARTE** — w sieci bez uprawnień brak taniego dowodu niezależności stron |
| Sybil w puli weryfikatorów | **OTWARTE** — stake sam tworzy plutokrację i nie dowodzi niezależnego sprzętu |
| Wzrost liczby stron widzących prompt | **OTWARTE** — verifier pogarsza §6 whitepapera, nie policzone |
| Odporność samego TopLoc na ataki poniżej top-k | **OTWARTE** — poza zakresem tego dokumentu (blocker P0 nr 1) |
| Polityka dla zleceń zbyt długich, by je zaudytować jednym przebiegiem | **OTWARTE** — odrzucać, przyjmować bez audytu, czy wymuszać niższy limit? Każda opcja ma cenę: odrzucanie ogranicza produkt, przyjmowanie tworzy klasę zleceń poza kontrolą |

### Pierwszy pomiar uczciwego dryfu — ZMIERZONE (2026-09-17)

Baseline na **jednej karcie, jednym modelu, jednym silniku** (Szpon, RTX 3090,
qwen3.8-27b, vLLM), `temperature=0`, ustalony `seed` — czyli **najlepszy możliwy
przypadek** dla powtarzalności. Proxy: tokeny + logprobs (API nie wystawia
aktywacji), 6 przebiegów, 400 porównań pozycji.

| Miara | Wynik |
|---|---|
| tokeny i tekst identyczne we wszystkich przebiegach | **TAK** |
| to samo przy **innym składzie batcha** (zlecenie solo vs w tłoku) | **TAK** — tokeny identyczne |
| pozycji bit-identycznych (Δ = 0) | **49%** |
| mediana Δ logprob | 1,19×10⁻⁷ |
| p90 / p99 / maks | 1,6×10⁻³ / 6,6×10⁻² / 7,4×10⁻² |
| pozycji z Δ > 10⁻³ | **49/400 (12%)** |

**Gdzie dryf siedzi:**

| | mediana marginesu top1–top2 (logity wyjściowe) |
|---|---|
| pozycje **z** dryfem > 10⁻³ | **1,25** |
| pozycje **bez** dryfu | **9,125** |

Dryf koncentruje się w pozycjach o **niskim marginesie logitów wyjściowych**.
Tam, gdzie margines jest duży, wynik jest powtarzalny co do bitu.

### Co z tego WYNIKA, a co jest dopiero hipotezą

**WYNIKA (i to wystarczy na decyzję):** identyczny tekst **nie oznacza**
identycznego śladu numerycznego, nawet w najlepszym przypadku sprzętowym.
**Bitowe porównanie wartości numerycznych jest wykluczone jako podstawa
slasha.** To jest mocny, zamknięty wynik.

**NIE WYNIKA — dwa zdania, które wcześniej postawiłem za mocno i wycofuję:**

1. ~~„Kryjówka atakującego i uczciwy szum to ten sam obszar"~~ →
   **HIPOTEZA, NIEZMIERZONA.** Zmierzyłem margines **logitów wyjściowych**
   (`logit(top1) − logit(top2)`). TopLoc pracuje na **marginesie granicy top-k
   aktywacji warstwy pośredniej** (`a_k − a_{k+1}`) i na osobnych statystykach
   różnic wykładników i mantys. To są **dwa różne marginesy**. Korelacja dryfu
   logprobów z pierwszym **nie dowodzi** korelacji stabilności receiptu TopLoc
   z drugim.
2. ~~„Żaden stały próg ich nie oddzieli"~~ → **NIEWYKAZANE.** Zmierzony został
   wyłącznie rozkład uczciwy `H_0`. Bez rozkładów kontrolowanych manipulacji
   `H_1` nie da się orzec o rozdzielności. Możliwe są trzy wyniki: rozkłady
   rozdzielne (próg istnieje), częściowo nakładające się (potrzebna szara strefa
   i eskalacja) albo praktycznie nierozdzielne (dana klasa ataku pozostaje
   nieweryfikowalna). **Nie wiemy który.**

**Poprawne sformułowanie na dziś:** stałego progu nie wolno ustalać wyłącznie
z maksimum dryfu; wyniki **sugerują** potrzebę tolerancji zależnej od marginesu,
ale jej zdolność odróżniania oszustwa od uczciwego wykonania jest
**niezmierzona**.

**Batch — też ostrożniej:** wolno napisać „w testowanej konfiguracji zmiana
obciążenia nie zmieniła tokenów ani tekstu". **Nie wolno** napisać „skład batcha
nie wpływa na wynik" — literatura pokazuje, że różne rozmiary batcha, wersje
i liczby GPU potrafią zmieniać wynik dekodowania zachłannego, a jądra
batch-invariant usuwają tylko część źródeł niedeterminizmu. Jeden stabilny
wynik to **dodatni punkt macierzy, nie zamknięcie osi**.

**Zastrzeżenia:** proxy (logprobs) zamiast aktywacji; jedna karta; jeden model;
jeden silnik; najlepszy przypadek (`temperature=0`, stały seed). **n = 400
pozycji to za mało na ogon** — p99 opiera się praktycznie na kilku największych
wartościach, więc jest **sygnałem, nie ustabilizowanym parametrem protokołu**.
Surowe: `tmp/m24v_dryf_rozklad.json`, `tmp/m24v_dryf_bazowy.json`.

### D1 na aktywacjach — ZROBIONE na Franku (2026-09-17), wynik ODWRACA obraz

Wykonane na **RTX 3080 Ti (sm_86)** u Franka — Szpon odpadał (3090 zajęta przez
vLLM, Titan X to sm_52 przy torchu wspierającym sm_75+). Model dekoderowy,
ścieżka **prefill**, hook na środkowej warstwie (jak zrobiłaby to implementacja
TopLoc), 8 przebiegów, top-128.

| Miara | Wynik |
|---|---|
| wartości top-k **bit-identyczne** | **100,00%** |
| mediana / p90 / p99 / maks dryfu | **0 / 0 / 0 / 0** |
| **zmiany indeksów top-k** | **0 / 409 472 (0,0000%)** |
| margines granicy `a_k − a_{k+1}` (mediana) | 4,88×10⁻³ |

**Aktywacje były idealnie powtarzalne** — w przeciwieństwie do logprobów, gdzie
51% pozycji się różniło.

### ⚠️ Ten pomiar jest OBCIĄŻONY — zmieniłem trzy rzeczy naraz

Między pomiarem logprobów a pomiarem aktywacji zmieniły się **trzy zmienne**:

| | pomiar logprobów (dryf 51%) | pomiar aktywacji (dryf 0%) |
|---|---|---|
| model | qwen3.8-27b | Qwen2.5-1.5B |
| kwantyzacja | W4A16 | fp16 |
| **stos serwujący** | **vLLM** (ciągły batching, grafy CUDA, paged attention) | **czysty forward transformers** (batch=1, stały kształt) |

**Nie wolno więc powiedzieć „aktywacje nie dryfują, a logprobs tak".** Wolno
powiedzieć tyle: w konfiguracji z ustalonym kształtem wsadu i tymi samymi
jądrami przy każdym przebiegu wynik był bit-identyczny.

**HIPOTEZA (testowalna, nie wynik):** źródłem zaobserwowanego dryfu jest
**stos serwujący**, nie karta ani model. Czysty forward o stałym kształcie jest
deterministyczny z konstrukcji; vLLM z dynamicznym batchingiem jest znanym
źródłem niedeterminizmu.

**Gdyby ta hipoteza się potwierdziła, ma to poważną konsekwencję projektową:**
protokół mógłby **narzucić kanoniczną ścieżkę weryfikacji** (ustalony kształt
wsadu, tryb eager zamiast grafów CUDA, deterministyczne jądra) i odzyskać
powtarzalność co do bitu. To zamieniłoby otwarty problem tolerancji w problem
ograniczony — i **częściowo cofnęłoby** wniosek „bitowe porównanie wykluczone",
który dotyczy logprobów raportowanych przez stos z dynamicznym batchingiem,
a niekoniecznie aktywacji liczonych ścieżką kanoniczną.

**D2 — eksperyment rozstrzygający (niezrobiony):** ten sam model, te same
wejścia, **dwie ścieżki** — raz przez vLLM, raz przez czysty forward — i
porównanie. Dopiero to izoluje zmienną. Bez D2 powyższe pozostaje hipotezą.

**Pozostałe zastrzeżenia D1:** jeden model (nie produkcyjny Bielik/AWQ), jedna
warstwa, tylko prefill (bez decode), batch=1 bez współobciążenia, 8 przebiegów.
Macierz A–G poniżej jest nadal niezrobiona. Surowe:
`tmp/m24v_d1_dryf_aktywacji.json`.

### Macierz, która pozostaje do zrobienia

Kolejnym krokiem **nie jest druga maszyna**, tylko zmierzenie na tej samej
karcie dokładnie tego, co TopLoc faktycznie weryfikuje. Dla wybranej warstwy:

- pełne aktywacje przed hashowaniem,
- **indeksy** top-k i liczba ich zmian między przebiegami,
- **wartości** top-k oraz margines granicy `a_k − a_{k+1}`,
- różnice wykładników i mantys — te same statystyki, których używa walidator
  TopLoc (osobne progi na liczbę niezgodnych wykładników oraz średnią i medianę
  różnic mantys),
- wynik PASS/FAIL dla progów z publikacji,
- **osobno prefill i decode.**

Macierz lokalna (jedna karta, jeden model), zanim ruszymy na drugi sprzęt:

```
A. ten sam request, izolowany, N powtórzeń
B. ten sam request, stały batch
C. zmienny rozmiar batcha
D. zmienny skład batcha
E. prefix cache OFF / ON
F. eager / CUDA graph
G. współuczestnicy batcha o różnych długościach
```

Dopiero po wyznaczeniu `H_0` **na aktywacjach** ma sens `H_1`: zmieniona
kwantyzacja, podmienione wagi, mała perturbacja niskiego rzędu, inny model tej
samej rodziny, manipulacja poniżej granicy top-k, manipulacja między
audytowanymi pozycjami, proxy do innego modelu. Dla każdej klasy: false accept,
false reject i sprawdzenie, czy `boundary margin` pozwala zbudować sensowną
strefę tolerancji.

**Gdzie to wykonano:** na Franku (3080 Ti, sm_86), nie na Szponie — 3090 była
zajęta przez vLLM (631 MiB wolnego), a Titan X to sm_52 przy torchu wspierającym
sm_75+. Na czas pomiaru zatrzymano dwa `llama-server` z eksperymentu LiteLLM
i **przywrócono je po zakończeniu**.

### Ryzyko, które może przewrócić cały kierunek

**Jeżeli zmierzony rozkład „uczciwego dryfu" okaże się szeroki, dokładne
porównanie fingerprintów może być w praktyce niewykonalne.** Różne
implementacje kwantyzacji, wersje CUDA, jądra obliczeniowe i silniki
(vLLM/llama.cpp/TensorRT) dają różne aktywacje dla tego samego modelu i wejścia.
Jeśli rozrzut uczciwych wyników przykryje sygnał oszustwa, to **nie ma progu,
który oddziela jedno od drugiego** — i wtedy problemem nie jest parametr, tylko
metoda.

Ten dokument **nie ma ścieżki odwrotu na ten przypadek** i nie udaje, że ma.
Pomiar rozkładu dryfu na macierzy sprzętowej (różne karty, sterowniki, silniki,
kwantyzacje) jest więc **pierwszym eksperymentem M2.5, nie formalnością** — bo
jego wynik rozstrzyga, czy TopLoc w tej sieci w ogóle ma sens, zanim ktokolwiek
zacznie projektować kary i kworum.

**Uboczny wniosek o przenośności:** wymóg „ten sam silnik i ta sama
kwantyzacja" ogranicza pulę weryfikatorów do node'ów o zgodnym stosie. Sieć
heterogenicznych domowych kart — czyli dokładnie to, czym SIMON ma być — jest
dla tej metody najtrudniejszym przypadkiem, nie najłatwiejszym.

**Status całości ekonomii verifiera: HIPOTEZA.** Jedyna pozycja o statusie
ZMIERZONE to koszt audytu (§2). Reszta wymaga symulacji i adversarialnego
testnetu, zanim powstanie linijka kodu.

---

## Co odblokowuje ten dokument, a co nie

**Odblokowuje:** pozycję „koszt jednego audytu zmierzony" z checklisty M2.4V —
z realną liczbą i nazwanymi zastrzeżeniami, nie szacunkiem z paperu.

**Nie odblokowuje M2.5.** Pozostałe pozycje M2.4V (commit/reveal, slash/appeal,
koluzja, test „zawsze PASS") mają tu **projekt albo jawne OTWARTE**, a nie
rozwiązanie. Zgodnie z zasadą projektu: bez dowodu nie ma statusu.
