//! M5.2a — trwała tożsamość klienta.
//!
//! **Dziura, którą to naprawia (znaleziona 2026-09-18):** agent wołał
//! `Keypair::generate()` przy KAŻDYM uruchomieniu. Dwa uruchomienia były więc
//! dwiema różnymi osobami, a `client_pubkey` w metryczniku M5.2 był za każdym
//! razem innym losowym kluczem — czyli rejestr nie potrafił przypisać pracy do
//! nikogo. Dowód z żywego pliku: 2 rekordy, 2 różne klucze klienta.
//!
//! **Czego tu celowo NIE MA:** frazy odzyskiwania, rotacji, unieważniania,
//! wyprowadzania wielu ról z jednego korzenia i czegokolwiek, co przypomina
//! portfel. To wymaga zamrożonego formatu i wektorów testowych (M5.2b/M5.2c
//! w ROADMAP). Tutaj naprawiamy wyłącznie fakt, że dwa uruchomienia są dziś
//! dwiema różnymi osobami.
//!
//! **Zasada nadrzędna: nigdy nie regenerujemy klucza po błędzie.** Cicha
//! regeneracja wyglądałaby jak udany start, a po cichu tworzyłaby nową
//! tożsamość i znowu rozcinała metrycznik — czyli ten sam błąd, tylko
//! trudniejszy do zauważenia.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::crypto::Keypair;

/// Świeże 32-bajtowe ziarno z CSPRNG systemowego.
///
/// Generujemy je TUTAJ, a nie wyciągamy z `Keypair` — `Keypair` celowo nie
/// wystawia sekretu i nie ma `Debug`, więc nie da się go wypisać przez pomyłkę.
/// Ta własność jest warta więcej niż wygoda jednej funkcji.
fn nowe_ziarno() -> [u8; 32] {
    use rand::RngCore;
    let mut z = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut z);
    z
}

/// Postać pliku tożsamości. `secret_encoding` jest jawne, żeby dało się zmienić
/// kodowanie bez zgadywania przy odczycie.
#[derive(Debug, Serialize, Deserialize)]
struct PlikTozsamosci {
    version: u32,
    key_type: String,
    /// `hex`, bo `hex` jest już zależnością projektu, a dokładanie base64 dla
    /// 32 bajtów nie ma uzasadnienia. Pole mówi, co jest w środku.
    secret_encoding: String,
    created_at: String,
    secret_key: String,
}

#[derive(Debug)]
pub enum BladTozsamosci {
    /// Plik istnieje, ale nie da się go odczytać jako tożsamości.
    /// **NIE generujemy wtedy nowego klucza.**
    Uszkodzony { sciezka: PathBuf, powod: String },
    /// Prawa dostępu pozwalają czytać sekret komuś poza właścicielem.
    ZlePrawa { sciezka: PathBuf, tryb: u32 },
    /// Nie da się zapisać — brak katalogu, brak praw, pełny dysk.
    NieMogeZapisac { sciezka: PathBuf, powod: String },
    /// Zapis się udał, ale odczyt zwrócił inny klucz publiczny.
    ZapisNiespojny { sciezka: PathBuf },
    BrakKataloguDanych,
}

impl std::fmt::Display for BladTozsamosci {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BladTozsamosci::Uszkodzony { sciezka, powod } => write!(
                f,
                "plik tożsamości {} jest uszkodzony ({powod}). NIE tworzę nowego klucza — \
                 to zmieniłoby Twoją tożsamość po cichu. Przywróć kopię albo usuń plik świadomie.",
                sciezka.display()
            ),
            BladTozsamosci::ZlePrawa { sciezka, tryb } => write!(
                f,
                "plik tożsamości {} ma prawa {:o} — sekret jest czytelny poza właścicielem. \
                 Napraw: chmod 600 {}",
                sciezka.display(),
                tryb,
                sciezka.display()
            ),
            BladTozsamosci::NieMogeZapisac { sciezka, powod } => {
                write!(f, "nie mogę zapisać tożsamości do {}: {powod}", sciezka.display())
            }
            BladTozsamosci::ZapisNiespojny { sciezka } => write!(
                f,
                "zapisałem {} , ale odczyt dał INNY klucz publiczny — przerywam",
                sciezka.display()
            ),
            BladTozsamosci::BrakKataloguDanych => {
                write!(f, "nie umiem ustalić katalogu danych użytkownika (HOME nie ustawiony?)")
            }
        }
    }
}

/// Domyślna ścieżka pliku tożsamości — katalog danych użytkownika, NIE repo
/// i NIE katalog bieżący.
pub fn domyslna_sciezka() -> Result<PathBuf, BladTozsamosci> {
    let dom = std::env::var_os("HOME").map(PathBuf::from);

    #[cfg(target_os = "macos")]
    let baza = dom.map(|h| h.join("Library/Application Support/SIMON"));

    #[cfg(target_os = "windows")]
    let baza = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|h| h.join("SIMON"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let baza = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| dom.map(|h| h.join(".local/share")))
        .map(|h| h.join("simon"));

    baza.map(|b| b.join("identity/client.key"))
        .ok_or(BladTozsamosci::BrakKataloguDanych)
}

/// Wczytuje trwałą tożsamość klienta albo zakłada ją przy pierwszym użyciu.
///
/// Semantyka błędu (świadoma, nie domyślna):
/// - brak pliku → wygeneruj i zapisz,
/// - poprawny plik → wczytaj,
/// - **uszkodzony plik → STOP, nie generuj nowego**,
/// - złe prawa → STOP,
/// - brak możliwości zapisu → STOP.
pub fn wczytaj_lub_zaloz(sciezka: &Path) -> Result<Keypair, BladTozsamosci> {
    if sciezka.exists() {
        return wczytaj(sciezka);
    }
    zaloz(sciezka)
}

fn wczytaj(sciezka: &Path) -> Result<Keypair, BladTozsamosci> {
    sprawdz_prawa(sciezka)?;
    let tekst = fs::read_to_string(sciezka).map_err(|e| BladTozsamosci::Uszkodzony {
        sciezka: sciezka.to_path_buf(),
        powod: e.to_string(),
    })?;
    let plik: PlikTozsamosci =
        serde_json::from_str(&tekst).map_err(|e| BladTozsamosci::Uszkodzony {
            sciezka: sciezka.to_path_buf(),
            powod: format!("nie parsuje się: {e}"),
        })?;
    if plik.key_type != "ed25519" || plik.secret_encoding != "hex" {
        return Err(BladTozsamosci::Uszkodzony {
            sciezka: sciezka.to_path_buf(),
            powod: format!("nieznany typ/kodowanie: {}/{}", plik.key_type, plik.secret_encoding),
        });
    }
    let raw = hex::decode(plik.secret_key.trim()).map_err(|e| BladTozsamosci::Uszkodzony {
        sciezka: sciezka.to_path_buf(),
        powod: format!("sekret nie jest hexem: {e}"),
    })?;
    let ziarno: [u8; 32] = raw.try_into().map_err(|_| BladTozsamosci::Uszkodzony {
        sciezka: sciezka.to_path_buf(),
        powod: "sekret nie ma 32 bajtów".into(),
    })?;
    Ok(Keypair::from_seed(&ziarno))
}

#[cfg(unix)]
fn sprawdz_prawa(sciezka: &Path) -> Result<(), BladTozsamosci> {
    use std::os::unix::fs::PermissionsExt;
    let tryb = fs::metadata(sciezka)
        .map_err(|e| BladTozsamosci::Uszkodzony {
            sciezka: sciezka.to_path_buf(),
            powod: e.to_string(),
        })?
        .permissions()
        .mode()
        & 0o777;
    // Cokolwiek poza właścicielem = sekret do wzięcia przez innego użytkownika.
    if tryb & 0o077 != 0 {
        return Err(BladTozsamosci::ZlePrawa { sciezka: sciezka.to_path_buf(), tryb });
    }
    Ok(())
}

#[cfg(not(unix))]
fn sprawdz_prawa(_sciezka: &Path) -> Result<(), BladTozsamosci> {
    Ok(())
}

fn zaloz(sciezka: &Path) -> Result<Keypair, BladTozsamosci> {
    let katalog = sciezka.parent().ok_or_else(|| BladTozsamosci::NieMogeZapisac {
        sciezka: sciezka.to_path_buf(),
        powod: "ścieżka bez katalogu".into(),
    })?;
    fs::create_dir_all(katalog).map_err(|e| BladTozsamosci::NieMogeZapisac {
        sciezka: sciezka.to_path_buf(),
        powod: e.to_string(),
    })?;

    let ziarno = nowe_ziarno();
    let klucz = Keypair::from_seed(&ziarno);
    let plik = PlikTozsamosci {
        version: 1,
        key_type: "ed25519".into(),
        secret_encoding: "hex".into(),
        created_at: znacznik_czasu(),
        secret_key: hex::encode(ziarno),
    };
    let tresc = serde_json::to_string_pretty(&plik).map_err(|e| BladTozsamosci::NieMogeZapisac {
        sciezka: sciezka.to_path_buf(),
        powod: e.to_string(),
    })?;

    // Zapis atomowy: plik tymczasowy W TYM SAMYM katalogu (żeby rename nie
    // przekraczał systemu plików) → fsync → prawa → rename. Przerwanie przed
    // rename zostawia śmieć, ale NIE niszczy poprzedniej tożsamości.
    let tymczasowy = katalog.join(format!(".client.key.{}.tmp", std::process::id()));
    let blad = |e: std::io::Error| BladTozsamosci::NieMogeZapisac {
        sciezka: sciezka.to_path_buf(),
        powod: e.to_string(),
    };
    {
        let mut f = fs::File::create(&tymczasowy).map_err(blad)?;
        f.write_all(tresc.as_bytes()).map_err(blad)?;
        f.sync_all().map_err(blad)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tymczasowy, fs::Permissions::from_mode(0o600)).map_err(blad)?;
    }
    fs::rename(&tymczasowy, sciezka).map_err(blad)?;

    // Odczytujemy z powrotem i porównujemy klucz publiczny. Zapis, który się
    // "udał", ale daje inną tożsamość, jest gorszy niż brak zapisu.
    let odczytany = wczytaj(sciezka)?;
    if odczytany.public() != klucz.public() {
        return Err(BladTozsamosci::ZapisNiespojny { sciezka: sciezka.to_path_buf() });
    }
    Ok(klucz)
}

fn znacznik_czasu() -> String {
    // Bez zależności od biblioteki dat: sekundy od epoki wystarczą jako
    // informacja porządkowa i NIE zasilają żadnej decyzji.
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{s}")
}

#[cfg(test)]
mod testy {
    use super::*;

    fn katalog() -> tempfile::TempDir {
        tempfile::tempdir().expect("katalog")
    }

    /// REGRESJA 2026-09-18 — dokładnie znaleziona dziura.
    /// Przed: 2 uruchomienia → 2 różne `client_pubkey`. Po: → 1.
    #[test]
    fn dwa_uruchomienia_daja_ten_sam_klucz() {
        let k = katalog();
        let p = k.path().join("identity/client.key");
        let a = wczytaj_lub_zaloz(&p).expect("pierwsze");
        let b = wczytaj_lub_zaloz(&p).expect("drugie");
        assert_eq!(a.public(), b.public(), "dwa uruchomienia to ma byc TA SAMA osoba");
    }

    #[test]
    fn rozne_katalogi_daja_rozne_klucze() {
        let (k1, k2) = (katalog(), katalog());
        let a = wczytaj_lub_zaloz(&k1.path().join("i/client.key")).unwrap();
        let b = wczytaj_lub_zaloz(&k2.path().join("i/client.key")).unwrap();
        assert_ne!(a.public(), b.public());
    }

    #[test]
    fn uszkodzony_plik_nie_powoduje_cichej_regeneracji() {
        let k = katalog();
        let p = k.path().join("client.key");
        fs::write(&p, "{to nie jest tozsamosc").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let wynik = wczytaj_lub_zaloz(&p);
        assert!(
            matches!(wynik, Err(BladTozsamosci::Uszkodzony { .. })),
            "uszkodzony plik MUSI zatrzymac start, a nie stworzyc nowa tozsamosc"
        );
        // I nie wolno go nadpisać.
        assert_eq!(fs::read_to_string(&p).unwrap(), "{to nie jest tozsamosc");
    }

    #[test]
    fn przerwany_zapis_nie_niszczy_poprzedniego_klucza() {
        let k = katalog();
        let p = k.path().join("client.key");
        let pierwotny = wczytaj_lub_zaloz(&p).unwrap();
        // Symulacja przerwania: śmieć po nieudanym zapisie zostaje w katalogu.
        fs::write(k.path().join(".client.key.999.tmp"), "polowa zapisu").unwrap();
        let po = wczytaj_lub_zaloz(&p).unwrap();
        assert_eq!(pierwotny.public(), po.public(), "tozsamosc ma przezyc smiec po przerwaniu");
    }

    #[cfg(unix)]
    #[test]
    fn nowy_plik_ma_prawa_600() {
        use std::os::unix::fs::PermissionsExt;
        let k = katalog();
        let p = k.path().join("client.key");
        wczytaj_lub_zaloz(&p).unwrap();
        let tryb = fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(tryb, 0o600, "sekret nie moze byc czytelny poza wlascicielem");
    }

    #[cfg(unix)]
    #[test]
    fn zbyt_szerokie_prawa_zatrzymuja_start() {
        use std::os::unix::fs::PermissionsExt;
        let k = katalog();
        let p = k.path().join("client.key");
        wczytaj_lub_zaloz(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(wczytaj_lub_zaloz(&p), Err(BladTozsamosci::ZlePrawa { .. })));
    }

    #[test]
    fn kopia_pliku_odtwarza_ten_sam_klucz_publiczny() {
        let (k1, k2) = (katalog(), katalog());
        let p1 = k1.path().join("client.key");
        let oryginal = wczytaj_lub_zaloz(&p1).unwrap();
        // "Druga maszyna" to po prostu inny katalog z tym samym plikiem.
        let p2 = k2.path().join("client.key");
        fs::copy(&p1, &p2).unwrap();
        assert_eq!(wczytaj_lub_zaloz(&p2).unwrap().public(), oryginal.public());
    }

    #[test]
    fn komunikat_bledu_nie_zawiera_sekretu() {
        let k = katalog();
        let p = k.path().join("client.key");
        wczytaj_lub_zaloz(&p).expect("zalozenie");
        let zapisany: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        let sekret = zapisany["secret_key"].as_str().unwrap().to_string();
        assert_eq!(sekret.len(), 64, "32 bajty jako hex");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
            match wczytaj_lub_zaloz(&p) {
                Err(e) => assert!(
                    !format!("{e}").contains(&sekret),
                    "komunikat bledu ujawnil sekret"
                ),
                Ok(_) => panic!("zbyt szerokie prawa mialy zatrzymac start"),
            }
        }
    }
}
