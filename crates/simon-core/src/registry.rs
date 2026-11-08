//! M5.2 — local append-only work registry.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::crypto::{PublicKey, Signature};

/// Verification status for a work record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    SignatureValid,
    OutputBound,
    ExecutionAuditPending,
    ExecutionAuditPass,
    ExecutionAuditFail,
}

pub type StatusWeryfikacji = VerificationStatus;

/// Single completed work record from the client's perspective.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkRecordV1 {
    pub schema: u32,
    pub receipt_hash: String,
    pub job_id: String,

    pub client_pubkey: PublicKey,
    pub executor_pubkey: PublicKey,

    pub model_declared: String,
    pub runtime_declared: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokenizer_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_profile_hash: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_commitment: Option<String>,
    pub output_commitment: String,

    pub prompt_tokens_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_computed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_cached: Option<u64>,
    pub completion_tokens: u64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_observed_ttft_ms: Option<u64>,
    pub client_observed_total_ms: u64,

    #[serde(default)]
    pub identity_epoch: u32,
    pub verification_status: VerificationStatus,
    pub receipt_signature: Signature,
}

pub type RekordPracyV1 = WorkRecordV1;

#[derive(Debug)]
pub enum RegistryError {
    Duplicate { receipt_hash: String },
    Io(std::io::Error),
    Serde(serde_json::Error),
    MissingDataDir,
}

pub type BladRejestru = RegistryError;

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Duplicate { receipt_hash } => {
                write!(f, "receipt {receipt_hash} already recorded in registry")
            }
            RegistryError::Io(e) => write!(f, "I/O error: {e}"),
            RegistryError::Serde(e) => write!(f, "JSON serialization error: {e}"),
            RegistryError::MissingDataDir => write!(f, "cannot determine data directory"),
        }
    }
}

pub struct WorkRegistry {
    path: PathBuf,
    seen_receipts: HashSet<String>,
}

pub type RejestrPracy = WorkRegistry;
pub type Rejestr = WorkRegistry;

impl WorkRegistry {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, RegistryError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(RegistryError::Io)?;
        }
        let mut seen = HashSet::new();
        if path.exists() {
            let file = OpenOptions::new()
                .read(true)
                .open(path)
                .map_err(RegistryError::Io)?;
            for line in BufReader::new(file).lines() {
                let line = line.map_err(RegistryError::Io)?;
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let record: WorkRecordV1 =
                    serde_json::from_str(trimmed).map_err(RegistryError::Serde)?;
                seen.insert(record.receipt_hash);
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            seen_receipts: seen,
        })
    }

    pub fn default_path() -> Result<PathBuf, RegistryError> {
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

        base.map(|b| b.join("registry/work_records.jsonl"))
            .ok_or(RegistryError::MissingDataDir)
    }

    pub fn append(&mut self, record: &WorkRecordV1) -> Result<(), RegistryError> {
        if self.seen_receipts.contains(&record.receipt_hash) {
            return Err(RegistryError::Duplicate {
                receipt_hash: record.receipt_hash.clone(),
            });
        }
        let json = serde_json::to_string(record).map_err(RegistryError::Serde)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(RegistryError::Io)?;
        writeln!(file, "{json}").map_err(RegistryError::Io)?;
        file.sync_all().map_err(RegistryError::Io)?;
        self.seen_receipts.insert(record.receipt_hash.clone());
        Ok(())
    }

    pub fn dopisz(&mut self, rekord: &WorkRecordV1) -> Result<(), RegistryError> {
        self.append(rekord)
    }

    pub fn otworz<P: AsRef<Path>>(sciezka: P) -> Result<Self, RegistryError> {
        Self::open(sciezka)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;

    fn sample_record(receipt_hash: &str) -> WorkRecordV1 {
        let client_kp = Keypair::from_seed(&[1u8; 32]);
        let executor_kp = Keypair::from_seed(&[2u8; 32]);
        let sig = executor_kp.sign_digest("digest");
        WorkRecordV1 {
            schema: 1,
            receipt_hash: receipt_hash.into(),
            job_id: "job-1".into(),
            client_pubkey: client_kp.public(),
            executor_pubkey: executor_kp.public(),
            model_declared: "model-1".into(),
            runtime_declared: "vllm".into(),
            model_manifest_hash: None,
            tokenizer_manifest_hash: None,
            execution_profile_hash: None,
            prompt_commitment: None,
            output_commitment: "output-hash".into(),
            prompt_tokens_total: 10,
            prompt_tokens_computed: None,
            prompt_tokens_cached: None,
            completion_tokens: 20,
            client_observed_ttft_ms: None,
            client_observed_total_ms: 100,
            identity_epoch: 1,
            verification_status: VerificationStatus::OutputBound,
            receipt_signature: sig,
        }
    }

    #[test]
    fn duplicate_receipt_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.jsonl");
        let mut reg = WorkRegistry::open(&path).unwrap();
        let r = sample_record("hash-1");
        assert!(reg.append(&r).is_ok());
        assert!(matches!(
            reg.append(&r),
            Err(RegistryError::Duplicate { .. })
        ));
    }

    #[test]
    fn duplicate_detected_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.jsonl");
        {
            let mut reg = WorkRegistry::open(&path).unwrap();
            reg.append(&sample_record("hash-2")).unwrap();
        }
        let mut reg2 = WorkRegistry::open(&path).unwrap();
        assert!(matches!(
            reg2.append(&sample_record("hash-2")),
            Err(RegistryError::Duplicate { .. })
        ));
    }

    #[test]
    fn missing_fields_serialize_as_none_not_zero() {
        let r = sample_record("hash-3");
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("prompt_tokens_cached"));
        assert!(!json.contains("prompt_commitment"));
    }
}
