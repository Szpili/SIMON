//! M3.x — chunkowanie dla promptów większych niż limit jednego node'a
//! (`docs/KV-CACHE-spill-i-rozproszony-VRAM.md`, sekcja 2 "Duży plik").
//!
//! Ten moduł to WYŁĄCZNIE czysta logika dzielenia tekstu — bez sieci, w pełni
//! testowalna. Orkiestracja map-reduce (wysyłka do node'a, redukcja
//! hierarchiczna, adaptacyjne dzielenie po `ctx_ponad_limit`) jest w
//! `agent.rs`, bo potrzebuje żywego połączenia p2p.
//!
//! **Dlaczego reduce MUSI być hierarchiczny (drzewiasty), nie jednostrzałowy:**
//! plan v1 zakładał jedno zlecenie reduce na WSZYSTKIE wyniki cząstkowe.
//! Krytyka planu (tmp/critique_plan_mapreduce.log) złapała, że dla właśnie
//! tego przypadku, dla którego ta warstwa istnieje (1 MB pliku, ~72 kawałki),
//! samo zlecenie reduce przekroczyłoby limit node'a. Stąd `grupuj_do_budzetu`:
//! pakuje wyniki cząstkowe w grupy mieszczące się w budżecie, agent.rs redukuje
//! poziomami aż zostanie jeden wynik (log-głębokość zamiast jednego przeskoku).

/// Najgęstszy dotąd zmierzony materiał (base64, docs/KV-CACHE sekcja 9,
/// 2026-09-17) — konserwatywny domyślny mnożnik znak/token przy szacowaniu
/// budżetu kawałka. To NIE jest gwarancja formalna (inny materiał mógłby być
/// gęstszy) — prawdziwa gwarancja to bramka C+1 na node'u (realny `/tokenize`)
/// + adaptacyjne dzielenie w agent.rs, gdy mimo to coś przekroczy limit.
pub const ZNAK_NA_TOKEN_WORST_CASE: f64 = 1.2;

/// Zmierzone znak/tok dla polskiej prozy (docs/KV-CACHE sekcja 7, sesja
/// 500-7800 tok. na Franko). Wyniki cząstkowe w fazie reduce to ZAWSZE tekst
/// wygenerowany przez model — nigdy surowy plik nieznanego typu — więc
/// budżet reduce liczy się tym mnożnikiem, nie `ZNAK_NA_TOKEN_WORST_CASE`.
/// Bez tego rozróżnienia: przy ciasnym `--chunk-tokens` budżet reduce (liczony
/// worst-case) jest za wąski, żeby zgrupować choćby 2 wyniki cząstkowe razem
/// w drugiej rundzie — złapane na żywo 2026-09-17 (test na Szponie, 20
/// kawałków, runda 2 nie mogła się zgrupować: 6 wyników -> 6 grup).
pub const ZNAK_NA_TOKEN_PROZA_GENEROWANA: f64 = 3.5;

/// Dzieli `tekst` na kawałki o długości ≤ `budzet_znakow` znaków. Cięcie
/// zawsze na granicy znaku (bezpieczne dla polskich diakrytyków i innych
/// wielobajtowych UTF-8 — `String` w Rust nigdy nie pozwoli przeciąć w
/// środku bajtu, ale trzeba jawnie ciąć po granicach `char`, nie bajtów).
///
/// Pusty tekst → pusty wynik (wołający ma to zgłosić jako błąd użytkownika,
/// nie próbować map-reduce na niczym).
pub fn podziel_na_kawalki(tekst: &str, budzet_znakow: usize) -> Vec<String> {
    if tekst.is_empty() || budzet_znakow == 0 {
        return Vec::new();
    }
    let znaki: Vec<char> = tekst.chars().collect();
    znaki
        .chunks(budzet_znakow)
        .map(|kawalek| kawalek.iter().collect())
        .collect()
}

/// Szacuje liczbę tokenów dla danego tekstu bez konieczności uruchamiania pełnego tokenizera.
///
/// Uwzględnia wielobajtowe znaki UTF-8 oraz granice słów:
/// `tekst.chars().count().div_ceil(4).max(tekst.split_whitespace().count())`.
#[allow(dead_code)]
pub fn oszacuj_tokeny(tekst: &str) -> usize {
    if tekst.is_empty() {
        return 0;
    }
    tekst
        .chars()
        .count()
        .div_ceil(4)
        .max(tekst.split_whitespace().count())
}

/// Pakuje `elementy` w kolejne grupy wg budżetu tokenów (oszacowanego przez `oszacuj_tokeny`).
#[allow(dead_code)]
pub fn grupuj_do_budzetu_tokenow(elementy: &[String], budzet_tokenow: usize) -> Vec<Vec<String>> {
    let mut grupy: Vec<Vec<String>> = Vec::new();
    let mut biezaca: Vec<String> = Vec::new();
    let mut biezace_tokeny = 0usize;

    for el in elementy {
        let tok = oszacuj_tokeny(el);
        if !biezaca.is_empty() && biezace_tokeny + tok > budzet_tokenow {
            grupy.push(std::mem::take(&mut biezaca));
            biezace_tokeny = 0;
        }
        biezace_tokeny += tok;
        biezaca.push(el.clone());
    }
    if !biezaca.is_empty() {
        grupy.push(biezaca);
    }
    grupy
}

/// Pakuje `elementy` w kolejne grupy, których SUMA długości (znaków) mieści
/// się w `budzet_znakow`, metodą first-fit sekwencyjnie (bez przestawiania
/// kolejności — kolejność wyników cząstkowych ma znaczenie dla spójności
/// odpowiedzi). Pojedynczy element PRZEKRACZAJĄCY budżet trafia do WŁASNEJ
/// grupy — nie dzielimy wyniku cząstkowego (to już jest wynik modelu, nie
/// surowy tekst), wołający musi to obsłużyć (patrz `agent.rs`: taka grupa
/// idzie do reduce pojedynczo, funkcjonalnie jako "przepchnięcie" bez redukcji).
pub fn grupuj_do_budzetu(elementy: &[String], budzet_znakow: usize) -> Vec<Vec<String>> {
    let mut grupy: Vec<Vec<String>> = Vec::new();
    let mut biezaca: Vec<String> = Vec::new();
    let mut biezaca_dlugosc = 0usize;

    for el in elementy {
        let dl = el.chars().count();
        if !biezaca.is_empty() && biezaca_dlugosc + dl > budzet_znakow {
            grupy.push(std::mem::take(&mut biezaca));
            biezaca_dlugosc = 0;
        }
        biezaca_dlugosc += dl;
        biezaca.push(el.clone());
    }
    if !biezaca.is_empty() {
        grupy.push(biezaca);
    }
    grupy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pusty_tekst_daje_pusty_wynik() {
        assert!(podziel_na_kawalki("", 100).is_empty());
    }

    #[test]
    fn zerowy_budzet_daje_pusty_wynik() {
        // Ochrona przed --chunk-tokens 0 -> budzet_znakow 0 -> nieskończona pętla gdzie indziej.
        assert!(podziel_na_kawalki("cokolwiek", 0).is_empty());
    }

    #[test]
    fn tekst_miesci_sie_w_jednym_kawalku() {
        let kawalki = podziel_na_kawalki("krótki tekst", 1000);
        assert_eq!(kawalki, vec!["krótki tekst".to_string()]);
    }

    #[test]
    fn tekst_dzieli_sie_na_n_kawalkow() {
        let tekst = "a".repeat(250);
        let kawalki = podziel_na_kawalki(&tekst, 100);
        assert_eq!(kawalki.len(), 3);
        assert_eq!(kawalki[0].len(), 100);
        assert_eq!(kawalki[1].len(), 100);
        assert_eq!(kawalki[2].len(), 50);
        assert_eq!(kawalki.concat(), tekst);
    }

    #[test]
    fn ciecie_bezpieczne_na_granicy_polskich_znakow() {
        // "ąćęłńóśźż" — same wielobajtowe znaki UTF-8 (2 bajty każdy w UTF-8).
        // Budżet 3 ZNAKI (nie bajty) — jeśli cięlibyśmy po bajtach, tekst by się
        // rozsypał (panika lub obcięty znak). Weryfikujemy roundtrip.
        let tekst = "ąćęłńóśźż".repeat(5);
        let kawalki = podziel_na_kawalki(&tekst, 3);
        assert_eq!(kawalki.concat(), tekst, "roundtrip musi być bezstratny");
        for k in &kawalki {
            assert!(k.chars().count() <= 3);
        }
    }

    #[test]
    fn grupowanie_pakuje_do_budzetu() {
        let elementy = vec!["aaaa".to_string(), "bbbb".to_string(), "cc".to_string()];
        // budżet 8: "aaaa"+"bbbb" = 8 (mieści się), "cc" osobno.
        let grupy = grupuj_do_budzetu(&elementy, 8);
        assert_eq!(grupy.len(), 2);
        assert_eq!(grupy[0], vec!["aaaa".to_string(), "bbbb".to_string()]);
        assert_eq!(grupy[1], vec!["cc".to_string()]);
    }

    #[test]
    fn grupowanie_zachowuje_kolejnosc() {
        let elementy: Vec<String> = (0..10).map(|i| format!("el{i}")).collect();
        let grupy = grupuj_do_budzetu(&elementy, 6);
        let splaszczone: Vec<String> = grupy.into_iter().flatten().collect();
        assert_eq!(splaszczone, elementy, "kolejność wyników cząstkowych nie może się zmienić");
    }

    #[test]
    fn element_wiekszy_niz_budzet_dostaje_wlasna_grupe() {
        let elementy = vec!["x".repeat(100)];
        let grupy = grupuj_do_budzetu(&elementy, 10);
        assert_eq!(grupy.len(), 1);
        assert_eq!(grupy[0].len(), 1);
    }

    #[test]
    fn duzo_elementow_daje_grupy_mniejsze_niz_calosc() {
        // To jest dokładnie test na błąd złapany w krytyce planu: reduce
        // jednostrzałowy na 72 elementach by przekroczył budżet. Grupowanie
        // MUSI dać >1 grupę, żeby hierarchiczna redukcja miała sens (agent.rs
        // sprawdza dodatkowo w runtime, że liczba grup < liczby elementów).
        let elementy: Vec<String> = (0..72).map(|_| "x".repeat(300)).collect(); // 72*300=21600 znaków
        let grupy = grupuj_do_budzetu(&elementy, 4000); // budżet jak jeden node
        assert!(grupy.len() > 1, "72 elementy po 300 znaków MUSZĄ się podzielić na >1 grupę przy budżecie 4000");
        assert!(grupy.len() < elementy.len(), "grupowanie musi redukować liczbę jednostek, inaczej drzewo nie kończy się");
    }

    #[test]
    fn test_oszacuj_tokeny_pusty_i_prosty_tekst() {
        assert_eq!(oszacuj_tokeny(""), 0);
        // "test" (4 znaki, 1 słowo) -> 4.div_ceil(4) = 1, words = 1 -> 1
        assert_eq!(oszacuj_tokeny("test"), 1);
        // "a b c d e" (9 znaków, 5 słów) -> 9.div_ceil(4) = 3, max(5) = 5
        assert_eq!(oszacuj_tokeny("a b c d e"), 5);
        // Długie słowo bez spacji: 19 znaków, 1 słowo -> 19.div_ceil(4) = 5, max(1) = 5
        assert_eq!(oszacuj_tokeny("superkalifragilisti"), 5);
    }

    #[test]
    fn test_oszacuj_tokeny_wielobajtowe_utf8_i_slowa() {
        // Polskie znaki wielobajtowe: "Zażółć gęślą jaźń"
        // 17 znaków char (mimo 31 bajtów w UTF-8!), 3 słowa.
        // 17.div_ceil(4) = 5, words = 3 -> 5.
        let tekst = "Zażółć gęślą jaźń";
        assert_eq!(oszacuj_tokeny(tekst), 5);
    }

    #[test]
    fn test_podziel_na_kawalki_wielobajtowe_utf8_i_emojis() {
        let tekst = "🚀zażółć🌟gęślą✨jaźń";
        let kawalki = podziel_na_kawalki(tekst, 3);
        assert_eq!(
            kawalki.concat(),
            tekst,
            "roundtrip z emoji i polskimi znakami musi być bezstratny"
        );
        for k in &kawalki {
            assert!(
                k.chars().count() <= 3,
                "każdy kawałek musi mieć co najwyżej 3 znaki"
            );
        }
    }

    #[test]
    fn test_grupuj_do_budzetu_wielobatchowe_zachowanie_kolejnosci() {
        // 25 elementów po 10 znaków = 250 znaków. Budżet 35 -> powstaje wiele grup,
        // a ich spłaszczenie musi w 100% zachować sekwencję 0..25.
        let elementy: Vec<String> = (0..25).map(|i| format!("batch_{i:04}")).collect();
        let grupy = grupuj_do_budzetu(&elementy, 35);
        assert!(grupy.len() > 1);
        let splaszczone: Vec<String> = grupy.into_iter().flatten().collect();
        assert_eq!(splaszczone, elementy);
    }

    #[test]
    fn test_grupuj_do_budzetu_tokenow_zachowuje_kolejnosc() {
        let elementy: Vec<String> = (0..20)
            .map(|i| format!("kawałek numer {i} z tekstem"))
            .collect();
        let grupy = grupuj_do_budzetu_tokenow(&elementy, 10);
        assert!(grupy.len() > 1);
        let splaszczone: Vec<String> = grupy.into_iter().flatten().collect();
        assert_eq!(splaszczone, elementy);
    }
}
