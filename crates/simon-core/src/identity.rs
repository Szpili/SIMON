//! M5.2a — persistent client identity.
//!
//! **Vulnerability fixed (found 2026-09-18):** the agent called
//! `Keypair::generate()` on EVERY launch. Two runs were therefore
//! two different identities, and `client_pubkey` in the M5.2 log was a different
//! random key each time — meaning the registry could not attribute work to
//! anyone. Proof from live log: 2 records, 2 different client keys.
//!
//! **Deliberately NOT here:** recovery phrases, key rotation, revocation,
//! deriving multiple roles from a single root, and anything resembling a wallet.
//! That requires a frozen format and test vectors (M5.2b/M5.2c in ROADMAP).
//! Here we solely ensure two runs are recognized as the same party.
//!
//! **Guiding rule: never regenerate keys on error.** Silent regeneration looks
//! like a successful start, but silently creates a new identity and splits the
//! ledger — reproducing the bug in a harder-to-spot way.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::crypto::Keypair;

/// Fresh 32-byte seed from the system CSPRNG.
///
/// Generated HERE rather than extracted from `Keypair` — `Keypair` intentionally
/// does not expose secret keys or implement `Debug`, preventing accidental leakage.
fn new_seed() -> [u8; 32] {
    use rand::RngCore;
    let mut s = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut s);
    s
}

/// On-disk identity file structure.
#[derive(Debug, Serialize, Deserialize)]
struct IdentityFile {
    version: u32,
    key_type: String,
    secret_encoding: String,
    created_at: String,
    secret_key: String,
}

#[derive(Debug)]
pub enum IdentityError {
    /// File exists but cannot be read as an identity.
    /// **Does NOT generate a new key.**
    Corrupt { path: PathBuf, reason: String },
    /// File permissions allow non-owner reading of the secret key.
    BadPermissions { path: PathBuf, mode: u32 },
    /// Cannot write: missing directory, insufficient permissions, disk full.
    CannotWrite { path: PathBuf, reason: String },
    /// Write succeeded but subsequent read returned a different public key.
    InconsistentWrite { path: PathBuf },
    MissingDataDir,
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::Corrupt { path, reason } => write!(
                f,
                "identity file {} is corrupt ({reason}). Refusing to create a new key — \
                 that would silently alter your identity. Restore a backup or delete the file deliberately.",
                path.display()
            ),
            IdentityError::BadPermissions { path, mode } => write!(
                f,
                "identity file {} has mode {:o} — secret is readable by others. \
                 Fix with: chmod 600 {}",
                path.display(),
                mode,
                path.display()
            ),
            IdentityError::CannotWrite { path, reason } => {
                write!(f, "cannot write identity to {}: {reason}", path.display())
            }
            IdentityError::InconsistentWrite { path } => write!(
                f,
                "wrote {}, but read-back gave a DIFFERENT public key — aborting",
                path.display()
            ),
            IdentityError::MissingDataDir => {
                write!(f, "cannot determine user data directory (HOME not set?)")
            }
        }
    }
}

pub type BladTozsamosci = IdentityError;

/// Default path for the identity file — user data directory, NOT repository or cwd.
pub fn default_path() -> Result<PathBuf, IdentityError> {
    let home = std::env::var_os("HOME").map(PathBuf::from);

    #[cfg(target_os = "macos")]
    let base = home.map(|h| h.join("Library/Application Support/SIMON"));

    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|h| h.join("SIMON"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".local/share")))
        .map(|h| h.join("simon"));

    base.map(|b| b.join("identity/client.key"))
        .ok_or(IdentityError::MissingDataDir)
}

pub use default_path as domyslna_sciezka;

/// Loads an existing persistent client identity or creates it on first use.
///
/// Error semantics:
/// - missing file -> generate and save,
/// - valid file -> load,
/// - corrupt file -> STOP, do not generate new,
/// - bad permissions -> STOP,
/// - write failure -> STOP.
pub fn load_or_create(path: &Path) -> Result<Keypair, IdentityError> {
    if path.exists() {
        return load(path);
    }
    create(path)
}

pub use load_or_create as wczytaj_lub_zaloz;

fn load(path: &Path) -> Result<Keypair, IdentityError> {
    check_permissions(path)?;
    let text = fs::read_to_string(path).map_err(|e| IdentityError::Corrupt {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    let file: IdentityFile =
        serde_json::from_str(&text).map_err(|e| IdentityError::Corrupt {
            path: path.to_path_buf(),
            reason: format!("failed to parse: {e}"),
        })?;
    if file.key_type != "ed25519" || file.secret_encoding != "hex" {
        return Err(IdentityError::Corrupt {
            path: path.to_path_buf(),
            reason: format!("unknown type/encoding: {}/{}", file.key_type, file.secret_encoding),
        });
    }
    let raw = hex::decode(file.secret_key.trim()).map_err(|e| IdentityError::Corrupt {
        path: path.to_path_buf(),
        reason: format!("secret key is not valid hex: {e}"),
    })?;
    let seed: [u8; 32] = raw.try_into().map_err(|_| IdentityError::Corrupt {
        path: path.to_path_buf(),
        reason: "secret key is not 32 bytes".into(),
    })?;
    Ok(Keypair::from_seed(&seed))
}

#[cfg(unix)]
fn check_permissions(path: &Path) -> Result<(), IdentityError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)
        .map_err(|e| IdentityError::Corrupt {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o077 != 0 {
        return Err(IdentityError::BadPermissions { path: path.to_path_buf(), mode });
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_permissions(_path: &Path) -> Result<(), IdentityError> {
    Ok(())
}

fn create(path: &Path) -> Result<Keypair, IdentityError> {
    let dir = path.parent().ok_or_else(|| IdentityError::CannotWrite {
        path: path.to_path_buf(),
        reason: "path has no parent directory".into(),
    })?;
    fs::create_dir_all(dir).map_err(|e| IdentityError::CannotWrite {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let seed = new_seed();
    let key = Keypair::from_seed(&seed);
    let file = IdentityFile {
        version: 1,
        key_type: "ed25519".into(),
        secret_encoding: "hex".into(),
        created_at: timestamp_str(),
        secret_key: hex::encode(seed),
    };
    let content = serde_json::to_string_pretty(&file).map_err(|e| IdentityError::CannotWrite {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    // Atomic write: temp file in same directory -> sync -> permissions 0600 -> rename.
    let temp_path = dir.join(format!(".client.key.{}.tmp", std::process::id()));
    let write_err = |e: std::io::Error| IdentityError::CannotWrite {
        path: path.to_path_buf(),
        reason: e.to_string(),
    };
    {
        #[cfg(unix)]
        let mut f = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temp_path)
                .map_err(write_err)?
        };
        #[cfg(not(unix))]
        let mut f = fs::File::create(&temp_path).map_err(write_err)?;

        f.write_all(content.as_bytes()).map_err(write_err)?;
        f.sync_all().map_err(write_err)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600)).map_err(write_err)?;
    }
    fs::rename(&temp_path, path).map_err(write_err)?;

    // Read back and verify public key match.
    let read_back = load(path)?;
    if read_back.public() != key.public() {
        return Err(IdentityError::InconsistentWrite { path: path.to_path_buf() });
    }
    Ok(key)
}

fn timestamp_str() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{s}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn two_runs_yield_same_key() {
        let d = temp_dir();
        let p = d.path().join("identity/client.key");
        let a = load_or_create(&p).expect("first");
        let b = load_or_create(&p).expect("second");
        assert_eq!(a.public(), b.public(), "two runs must produce the same identity");
    }

    #[test]
    fn different_dirs_yield_different_keys() {
        let (d1, d2) = (temp_dir(), temp_dir());
        let a = load_or_create(&d1.path().join("i/client.key")).unwrap();
        let b = load_or_create(&d2.path().join("i/client.key")).unwrap();
        assert_ne!(a.public(), b.public());
    }

    #[test]
    fn corrupt_file_prevents_silent_regeneration() {
        let d = temp_dir();
        let p = d.path().join("client.key");
        fs::write(&p, "{not a valid identity").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let res = load_or_create(&p);
        assert!(
            matches!(res, Err(IdentityError::Corrupt { .. })),
            "corrupt file must halt execution rather than regenerating"
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), "{not a valid identity");
    }

    #[test]
    fn interrupted_write_preserves_previous_key() {
        let d = temp_dir();
        let p = d.path().join("client.key");
        let initial = load_or_create(&p).unwrap();
        fs::write(d.path().join(".client.key.999.tmp"), "partial write").unwrap();
        let after = load_or_create(&p).unwrap();
        assert_eq!(initial.public(), after.public(), "identity must survive leftover temp file");
    }

    #[cfg(unix)]
    #[test]
    fn new_file_has_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let d = temp_dir();
        let p = d.path().join("client.key");
        load_or_create(&p).unwrap();
        let mode = fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "secret file permissions must be 0600");
    }

    #[cfg(unix)]
    #[test]
    fn overly_broad_permissions_halt_execution() {
        use std::os::unix::fs::PermissionsExt;
        let d = temp_dir();
        let p = d.path().join("client.key");
        load_or_create(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(load_or_create(&p), Err(IdentityError::BadPermissions { .. })));
    }

    #[test]
    fn file_copy_reproduces_same_public_key() {
        let (d1, d2) = (temp_dir(), temp_dir());
        let p1 = d1.path().join("client.key");
        let original = load_or_create(&p1).unwrap();
        let p2 = d2.path().join("client.key");
        fs::copy(&p1, &p2).unwrap();
        assert_eq!(load_or_create(&p2).unwrap().public(), original.public());
    }

    #[test]
    fn error_message_does_not_contain_secret() {
        let d = temp_dir();
        let p = d.path().join("client.key");
        load_or_create(&p).expect("create");
        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        let secret = saved["secret_key"].as_str().unwrap().to_string();
        assert_eq!(secret.len(), 64, "32 bytes encoded as hex");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
            match load_or_create(&p) {
                Err(e) => assert!(
                    !format!("{e}").contains(&secret),
                    "error message leaked secret key"
                ),
                Ok(_) => panic!("overly broad permissions should have stopped startup"),
            }
        }
    }
}
