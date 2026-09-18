# Pazur (macOS) — SIMON zbudowany, przetestowany i zweryfikowany (2026-09-17)

**SIMON działa na macOS.** Zbudowany, przetestowany, interop z Szponem
potwierdzony na żywo. Poniżej stan faktyczny, nie plan.

## Sprzęt i system — sprawdzone, nie założone

| Co | Rzeczywistość |
|---|---|
| Maszyna | MacBook Pro, **Intel Core i7-3720QM** (Ivy Bridge, 2012), 16 GB RAM, 8 rdzeni |
| Architektura | **x86_64** — NIE Apple Silicon (pierwsza wersja tego dokumentu zakładała `aarch64`, **błędnie**) |
| System | macOS **12.7.6** (Monterey), build 21H1320 |
| Toolchain | Xcode CLT obecne (`/Library/Developer/CommandLineTools`), Apple clang 14.0.0 — linker był, nie trzeba było nic instalować przez GUI |
| Rust | **nie było** — doinstalowany `rustup` (stable 1.98.1) |
| Zasilanie | AC, bateria 100% — `caffeinate` działa (na baterii z zamkniętą klapą nie zadziałałby żaden trik userland) |

**Konsekwencja korekty architektury:** skoro Pazur jest x86_64 tak jak Szpon,
test determinizmu formatu jest **cross-OS (Linux ↔ macOS), NIE cross-arch**.
Inna libc, inne syscalle, inny linker — ale ta sama endianness i ten sam
rozmiar słowa. To nadal łapie klasę błędów zależnych od systemu, ale NIE
domyka pytania o architekturę big-endian/ARM. Uczciwie: część pytania została
otwarta.

## Dostęp — pułapki, które kosztowały czas

1. **`tailscale status` potrafi kłamać o żywej maszynie.** Pokazywał
   `offline, last seen 25m ago` (licznik rósł), a maszyna żyła — `tailscale
   ping` wymusił odkrycie trasy i dostał `pong ... via <lan-host> in 186ms`,
   po czym `status` sam przeskoczył na `active; direct`. **Przy diagnozie
   dostępności: `tailscale ping` mówi prawdę wcześniej niż `tailscale status`.**
2. **macOS Remote Login jest WYŁĄCZONY** (`Connection refused` na LAN:22),
   ale **Tailscale SSH działa** — wejście idzie przez tailnet, nie przez
   natywne sshd.
3. **Konto to `user`** (nie `trebusz`) — `tailscale: failed to look up local
   user "trebusz"` to właśnie ta pomyłka.
4. **Tailscale SSH żąda potwierdzenia w przeglądarce** (`checkPeriod` w ACL):
   połączenie WISI z komunikatem `# To authenticate, visit: https://...`
   dopóki człowiek nie kliknie. To nie jest zwis ani sen maszyny —
   `ssh -vv` pokazuje, że handshake przechodzi i utyka dopiero na
   `SSH2_MSG_SERVICE_ACCEPT`. Trzeba trzymać połączenie otwarte podczas
   klikania (check domyka się na tej samej sesji).

## Dostarczenie kodu

Nie klonowaliśmy z Pazura (nie wiadomo, czy ma klucz do Szpona) — wypchnięte
**ze Szpona przez tar-over-ssh**, bo Tailscale SSH bez problemu obsługuje
zwykły kanał exec (SFTP/scp bywa kapryśne):

```bash
# na Szponie
cd ~/SIMON && tar czf - --exclude=target --exclude=docs/refs --exclude=tmp --exclude=.git . \
  | ssh user@<node-macos> 'mkdir -p ~/SIMON && tar xzf - -C ~/SIMON'
```
Ładunek: **0,2 MB** (całe repo z `target/` to 9,7 GB — wykluczenie obowiązkowe).

## Pierwsze przejście — profil `dev` (release niżej, też zrobiony)

Zaczęliśmy od `dev`, bo na CPU z 2012 r. `release` z naszego profilu
(`lto = true`, `codegen-units = 1`) zapowiadał się długo, a **determinizm
formatu nie zależy od optymalizacji**. (Szacunek okazał się przesadzony —
patrz sekcja o `--release` niżej: 7,5 min.)

```bash
# na Pazurze, odczepione od sesji SSH (sen/zerwanie nie ubije roboty)
nohup caffeinate -dimsu bash ~/simon_bringup.sh > ~/simon_bringup.log 2>&1 &
```

### Wynik: 19 binarek testowych, **0 porażek, 0 paniki**

Kluczowe — dokładnie te testy, które w przeszłości łapały rodzinę „format
wygląda tak samo, a nie jest" (`DefaultHasher` → różne `job_id` na różnych
`rustc`; `f64` w podpisywanej treści → różny digest po CBOR):

| Test | Wynik na macOS |
|---|---|
| `digest_is_deterministic` | ok |
| `signature_excluded_from_digest` | ok |
| `m26_receipt_przezywa_roundtrip_json` (padał przed poprawką `f64`) | ok |
| `job_id_jest_deterministyczny` | ok |
| `tampered_content_rejected`, `signed_receipt_verifies_self` | ok |
| cały `f3_receipt.rs` (18 testów) | ok |

Surowe: `tmp/pazur/testy_macos.log`.

## Interop na żywo — sedno

**Agent na macOS → node na Linuksie** (Szpon, qwen3.8-27b przez vLLM), przez
Tailscale:

```
=== WYNIK ===  (odpowiedź modelu)
Sieć (roundtrip): 5148 ms
Weryfikacja:      receipt spójny
```
Czyli **receipt podpisany Ed25519 na x86_64 Linux zweryfikował się na macOS** —
kanoniczna serializacja i podpis są zgodne między systemami.
Surowe: `tmp/pazur/pazur_interop.log`.

### Nowe z 2026-09-17, przepuszczone z Maca

- **map-reduce** (`tmp/pazur/pazur_mapreduce.log`): plik 2281 znaków → 6
  kawałków, redukcja 6→1, RC=0.
- **łańcuch B2** (`tmp/pazur/pazur_b2.log`): etap A → etap B, oba receipty
  spójne, RC=0. Model etapu B sam zauważył, że wynik etapu A jest ucięty —
  czyli dane faktycznie przepłynęły.

## Potwierdzenie poprawki przenośności `/tmp` → `std::env::temp_dir()`

Checkpointy map-reduce na Macu wylądowały w:
```
/var/folders/xp/712rc1sj1j1d8kkcbq4cwhm00000gn/T/simon-mapreduce-67705/
```
czyli w per-user katalogu macOS, nie w `/tmp`. **I tu jest niespodzianka warta
zapamiętania:** `$TMPDIR` w nieinteraktywnej sesji SSH jest **pusty**, a mimo to
trafiło we właściwe miejsce — bo `std::env::temp_dir()` na Darwinie sięga po
`confstr(_CS_DARWIN_USER_TEMP_DIR)`, gdy zmiennej nie ma. Gdyby to było
napisane „sprytniej", jako `env::var("TMPDIR").unwrap_or("/tmp")`, **spadłoby na
`/tmp`** — czyli dokładnie ten błąd, który poprawka miała usunąć. Stdlib zrobił
to lepiej niż ręczna wersja.

## Build `--release` na Macu — ZROBIONE, i szybciej niż szacowałem

Szacowałem 30-90 min (LTO + `codegen-units = 1` na CPU z 2012 r.).
**Rzeczywistość: 448 s = 7 min 28 s**, `rc=0`.

| Co | Wynik |
|---|---|
| `cargo build --release` | **448 s (7,5 min)**, rc=0 |
| Binarka | `target/release/simon`, **10,6 MB**, `Mach-O 64-bit executable x86_64` |
| `cargo test --release` | **306 s (5 min)**, rc=0 — **19 binarek, te same liczby co w `dev`**, 0 porażek, 0 paniki |
| Interop na binarce release | agent (macOS, release) → node (Linux): **`receipt spójny`** |

Czyli **optymalizacja i LTO niczego nie zmieniają w zachowaniu protokołu** —
wyniki testów w `release` są co do liczby identyczne z `dev`. Surowe:
`tmp/pazur/release_build_i_testy.log`, `tmp/pazur/pazur_interop_release.log`.

**Praktyczna uwaga:** build i testy szły odczepione
(`nohup caffeinate -dimsu ... &`) — przeżyły zamknięcie sesji SSH, bez
niespodzianek znanych z WSL na Franku.

## Zapora macOS i node NA Macu — sprawdzone

Uruchomienie `--role node` na Pazurze (gniazdo nasłuchujące, `--model-url`
celujący w vLLM Szpona przez Tailscale):

**Zapora NIE zapytała — zablokowała po cichu.** Stan wyjściowy: zapora
włączona, binarka `simon` **niepodpisana w ogóle** (`code object is not signed
at all`), `socketfilterfw --getappblocked` → *„The application is not part of
the firewall"* (ani zezwolenie, ani blokada — po prostu brak wpisu).

**Test rozstrzygający, który oddziela zaporę od ACL Tailscale:**

| Ścieżka | Przed wyjątkiem | Po wyjątku |
|---|---|---|
| loopback `127.0.0.1:9171` | **succeeded** | succeeded |
| LAN `<lan-host>:9171` (omija Tailscale) | **timeout** | succeeded |
| Tailscale `<node-macos>:9171` | **timeout** | succeeded |

Skoro **LAN też padał**, to nie było ACL Tailscale — LAN omija tailnet
całkowicie. A **timeout zamiast `refused`** rozstrzyga między *zaporą, która
milczy* a *usługą, której nie ma*: wyłączony Remote Login w tej samej sesji
dawał `Connection refused` natychmiast.

**Wniosek architektoniczny (asymetria, nie szczegół):** macOS jest bez tarcia
jako **klient** (agent — tylko połączenia wychodzące), ale jako **worker**
(node — nasłuch) **wymaga jawnego wyjątku w zaporze**. Każdy Mac w roli node'a
będzie wymagał jednego hasła administratora:

```bash
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add    <ścieżka>/simon
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp <ścieżka>/simon
```

## Interop w DRUGĄ stronę — receipt podpisany na macOS, zweryfikowany na Linuksie

Po otwarciu zapory: **agent (Szpon/Linux) → node (Pazur/macOS) → vLLM (Szpon)**.

```
[node na macOS] /tokenize: 17 tok. realnie vs 16 deklarowane (błąd +6%)
[node na macOS] policzone: 40 tokenów, podpisane kluczem 95c90351e3a4…
[agent na Linuksie] Weryfikacja: receipt spójny
```

Czyli podpis Ed25519 **złożony na macOS weryfikuje się na Linuksie** — razem z
wcześniejszym testem (podpis z Linuksa weryfikowany na macOS) **pętla interop
jest zamknięta w obie strony**. `order_id` policzony na Linuksie zgadza się co
do znaku z tym, który node na macOS odesłał w receipcie.

Uboczna obserwacja o topologii: TTFT 15,6 s, bo żądanie idzie Szpon→Pazur
(Tailscale), potem Pazur→Szpon po model (Tailscale znowu) i z powrotem —
**node daleko od swojego GPU płaci podwójne przejście przez sieć.**
Surowe: `tmp/pazur/interop_reverse.log`.

## Co zostało otwarte

- **Cross-arch** (ARM/Apple Silicon, big-endian) — nadal niesprawdzone, patrz
  korekta na górze. Pazur jest x86_64, więc to pytanie zostaje.


---

## 2026-09-18 — weryfikacja międzyplatformowa potwierdzona

**Sprzęt:** MacBook Pro, Intel Core i7-3720QM (2012), macOS 12.7.6, x86_64.
Nie Apple Silicon — publiczne README twierdziło inaczej i zostało poprawione.

**Build:** `cargo build --release` z czystego klona z GitHuba — **8 min 58 s**,
Mach-O 64-bit x86_64, 11,2 MB. To była PIERWSZA kompilacja gałęzi
`#[cfg(target_os = "macos")]` w historii projektu; do tego dnia kompilator
nigdy jej nie oglądał, bo CI chodziło wyłącznie na Linuksie.

**Tożsamość klienta (M5.2a) na macOS — działa, nie tylko się kompiluje:**

| | |
|---|---|
| Ścieżka | `~/Library/Application Support/SIMON/identity/client.key` |
| Prawa | `-rw-------` (0600) |
| Dwa uruchomienia | ten sam klucz |

**Weryfikacja receiptu wystawionego przez RTX 3090 (Szpon, vLLM):**

```
podpis Ed25519 : OK
treść wyniku   : zgodna z odciskiem
werdykt        : RECEIPT WAŻNY          (kod 0)

# ta sama odpowiedź + JEDNA spacja
treść wyniku   : NIEZGODNA — to nie jest ten wynik
werdykt        : ODRZUCONY              (kod 1)
```

Laptop bez GPU, trzynaście lat starszy od karty, która wykonała pracę,
sprawdza ją offline. **To jest teza architektoniczna projektu wykonana,
a nie opisana.**

## PUŁAPKA: Tailscale SSH na macOS zawsze zwraca kod 0

`exit 7` → 0. `false` → 0. `test -f /nie/ma` → 0. Zwykły SSH na Windows
zwraca uczciwe 7. Transport i `stdout` działają — kłamie wyłącznie kod wyjścia.

Każda automatyzacja warunkowana kodem wyjścia **cicho odwraca sens bramki**:
obserwator budowania dwa razy ogłosił sukces, gdy trwała jeszcze kompilacja
zależności. Obejście: znacznik w `stdout`, sprawdzany lokalnie.

```bash
ssh user@host "test -f ~/x && echo JEST || echo NIEMA" | grep -q JEST
```
