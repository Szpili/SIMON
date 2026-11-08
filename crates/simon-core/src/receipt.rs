//! Execution receipt — frozen format.
//!
//! Fix F3 (red-team RT1): "complete() does not validate signature"
//! -> 100% forgeable settlements, 0% detection.
//!
//! Three verification gates (Variant C):
//!   1. valid signature under the receipt key,
//!   2. receipt key == key REGISTERED for this node,
//!   3. receipt belongs to THIS job and THIS node (field match).

use serde::{Deserialize, Serialize};

use crate::crypto::{Keypair, PublicKey, Signature};
use crate::{content_digest, SimonError};

/// Computation precision (one quantization per cluster).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Bf16,
    Fp16,
    Fp32,
    Fp8,
    Fp4,
    /// Integer weight quantization (e.g. GGUF Q8_0, W8A8) — not fp8.
    Int8,
    /// 4-bit integer weight quantization (GGUF Q4_*, AWQ, GPTQ) — not fp4.
    Int4,
    /// Node cannot determine precision from model name.
    Unknown,
}

/// M1 bindings: input (tokenizer, nonce, prompt tokens) + state (sampling params, seed)
/// + token output digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct M1Bindings {
    pub schema_version: u32,
    pub receipt_level: u8,
    /// Execution profile for audit (e.g. "llama.cpp/Q4_K_M/cpu").
    pub exec_profile: String,
    pub tokenizer_hash: String,
    pub prompt_digest: String,
    pub output_token_chain: String,
    pub client_nonce: String,
    pub sampling_params_hash: String,
    pub rng_seed: u64,
}

pub type WiazaniaM1 = M1Bindings;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub job_id: String,
    pub node_id: String,
    /// Model hash (mismatch = different cluster).
    pub model_hash: String,
    /// Runtime version (mismatch = excluded from pool).
    pub runtime: String,
    pub precision: Precision,
    /// TopLoc activation LSH (258 B / 32 tokens, arXiv 2501.16007).
    pub activation_hash: String,
    /// Result content digest bound to `job_id`.
    pub output_digest: String,
    /// Actual work performed — signed for billing settlement.
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    /// Start and finish timestamps in microseconds.
    pub started_at_us: u64,
    pub finished_at_us: u64,
    /// Node public key (signed part of content).
    pub signer: PublicKey,
    /// Signature over content digest. Excluded from content digest.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<Signature>,

    /// M1: optional input/state bindings. Serialized as `bindings` with `wiazania` alias.
    #[serde(rename = "bindings", alias = "wiazania", skip_serializing_if = "Option::is_none", default)]
    pub bindings: Option<M1Bindings>,
}

// Backward alias for field access in transition
impl Receipt {
    pub fn wiazania(&self) -> Option<&M1Bindings> {
        self.bindings.as_ref()
    }
}

/// Domain separator for output digest.
pub const OUTPUT_DIGEST_DOMAIN: &str = "SIMON/OUTPUT/v1";
pub const DOMENA_ODCISKU_WYJSCIA: &str = OUTPUT_DIGEST_DOMAIN;

// --- M1 domain separators ---
pub const PROMPT_DOMAIN: &str = "SIMON/PROMPT/v1";
pub const DOMENA_PROMPTU: &str = PROMPT_DOMAIN;

pub const CHAIN_DOMAIN: &str = "SIMON/CHAIN/v1";
pub const DOMENA_LANCUCHA: &str = CHAIN_DOMAIN;

pub const SAMPLING_DOMAIN: &str = "SIMON/SAMPLING/v1";
pub const DOMENA_PARAMETROW: &str = SAMPLING_DOMAIN;

pub const TOKENIZER_DOMAIN: &str = "SIMON/TOKENIZER/v1";
pub const DOMENA_TOKENIZERA: &str = TOKENIZER_DOMAIN;

pub const SCHEMA_VERSION_M1: u32 = 2;
pub const RECEIPT_LEVEL_M1: u8 = 1;

/// Computes the output digest bound to a job_id.
pub fn output_digest_for(job_id: &str, output: &str) -> Result<String, SimonError> {
    content_digest(&serde_json::json!({
        "domena": OUTPUT_DIGEST_DOMAIN,
        "job_id": job_id,
        "output": output,
    }))
}

pub use output_digest_for as odcisk_wyjscia;

/// Computes the input prompt digest: tokenizer + client nonce + prompt_token_ids.
pub fn prompt_digest(
    tokenizer_hash: &str,
    client_nonce: &str,
    prompt_token_ids: &[u32],
) -> Result<String, SimonError> {
    content_digest(&serde_json::json!({
        "domena": PROMPT_DOMAIN,
        "tokenizer_hash": tokenizer_hash,
        "nonce": client_nonce,
        "tokens": prompt_token_ids,
    }))
}

pub use prompt_digest as odcisk_promptu;

/// Computes the token output chain digest bound to prompt digest.
pub fn token_chain_digest(
    prompt_digest: &str,
    output_token_ids: &[u32],
) -> Result<String, SimonError> {
    content_digest(&serde_json::json!({
        "domena": CHAIN_DOMAIN,
        "prompt": prompt_digest,
        "tokens": output_token_ids,
    }))
}

pub use token_chain_digest as lancuch_tokenow;

/// Computes sampling parameter digest.
pub fn params_digest(
    temperature_milli: i64,
    top_p_milli: i64,
    top_k: u32,
    min_p_milli: i64,
    repeat_penalty_milli: i64,
) -> Result<String, SimonError> {
    content_digest(&serde_json::json!({
        "domena": SAMPLING_DOMAIN,
        "temperature_milli": temperature_milli,
        "top_p_milli": top_p_milli,
        "top_k": top_k,
        "min_p_milli": min_p_milli,
        "repeat_penalty_milli": repeat_penalty_milli,
    }))
}

pub use params_digest as odcisk_parametrow;

/// Canonical sampling commitment structure without floats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplingCommitment {
    pub temperature_x1e6: u32,
    pub top_k: u16,
    pub top_p_x1e6: u32,
    pub min_p_x1e6: u32,
    pub typical_p_x1e6: u32,
    pub repeat_penalty_x1e6: u32,
    pub frequency_penalty_x1e6: u32,
    pub presence_penalty_x1e6: u32,
    pub mirostat_version: u8,
    pub mirostat_tau_x1e6: u32,
    pub mirostat_eta_x1e6: u32,
}

impl SamplingCommitment {
    pub fn digest(&self) -> Result<String, SimonError> {
        content_digest(&serde_json::json!({
            "domena": SAMPLING_DOMAIN,
            "commitment": self,
        }))
    }

    pub fn odcisk(&self) -> Result<String, SimonError> {
        self.digest()
    }

    pub fn greedy() -> Self {
        Self {
            temperature_x1e6: 0,
            top_k: 1,
            top_p_x1e6: 1_000_000,
            min_p_x1e6: 0,
            typical_p_x1e6: 1_000_000,
            repeat_penalty_x1e6: 1_000_000,
            frequency_penalty_x1e6: 0,
            presence_penalty_x1e6: 0,
            mirostat_version: 0,
            mirostat_tau_x1e6: 5_000_000,
            mirostat_eta_x1e6: 100_000,
        }
    }
}

/// Tokenizer digest.
pub fn tokenizer_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(TOKENIZER_DOMAIN.as_bytes());
    h.update([0u8]);
    h.update(bytes);
    hex::encode(h.finalize())
}

pub use tokenizer_digest as odcisk_tokenizera;

impl Receipt {
    /// Checks if this receipt matches the given output text.
    pub fn matches_output(&self, output: &str) -> bool {
        match output_digest_for(&self.job_id, output) {
            Ok(d) => d == self.output_digest,
            Err(_) => false,
        }
    }

    pub fn zgodny_z_wyjsciem(&self, output: &str) -> bool {
        self.matches_output(output)
    }

    /// Proof level of this receipt (0 = legacy, 1 = M1).
    pub fn level(&self) -> u8 {
        self.bindings.as_ref().map(|w| w.receipt_level).unwrap_or(0)
    }

    pub fn poziom(&self) -> u8 {
        self.level()
    }

    /// Builder adding M1 bindings.
    #[allow(clippy::too_many_arguments)]
    pub fn with_m1_bindings(
        mut self,
        tokenizer_hash: String,
        client_nonce: String,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
        sampling_params_hash: String,
        rng_seed: u64,
        exec_profile: String,
    ) -> Result<Self, SimonError> {
        let pd = prompt_digest(&tokenizer_hash, &client_nonce, prompt_token_ids)?;
        let lc = token_chain_digest(&pd, output_token_ids)?;
        self.bindings = Some(M1Bindings {
            schema_version: SCHEMA_VERSION_M1,
            receipt_level: RECEIPT_LEVEL_M1,
            exec_profile,
            tokenizer_hash,
            prompt_digest: pd,
            output_token_chain: lc,
            client_nonce,
            sampling_params_hash,
            rng_seed,
        });
        Ok(self)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn z_wiazaniem_m1(
        self,
        tokenizer_hash: String,
        client_nonce: String,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
        sampling_params_hash: String,
        rng_seed: u64,
        exec_profile: String,
    ) -> Result<Self, SimonError> {
        self.with_m1_bindings(
            tokenizer_hash,
            client_nonce,
            prompt_token_ids,
            output_token_ids,
            sampling_params_hash,
            rng_seed,
            exec_profile,
        )
    }

    /// Verifies M1 bindings.
    pub fn verify_m1(
        &self,
        tokenizer_hash: &str,
        client_nonce: &str,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
    ) -> Result<(), SimonError> {
        self.verify_self()?;
        let w = self.bindings.as_ref().ok_or_else(|| {
            SimonError::ReceiptMismatch("no M1 bindings (proof level 0)".into())
        })?;
        if w.receipt_level < RECEIPT_LEVEL_M1 {
            return Err(SimonError::ReceiptMismatch(
                "receipt_level below M1".into(),
            ));
        }
        if w.schema_version != SCHEMA_VERSION_M1 {
            return Err(SimonError::ReceiptMismatch(
                "unsupported schema_version for M1".into(),
            ));
        }
        if w.tokenizer_hash != tokenizer_hash {
            return Err(SimonError::ReceiptMismatch(
                "tokenizer_hash does not match the receipt".into(),
            ));
        }
        if w.client_nonce != client_nonce {
            return Err(SimonError::ReceiptMismatch("client_nonce does not match".into()));
        }
        let expected_pd = prompt_digest(tokenizer_hash, client_nonce, prompt_token_ids)?;
        if w.prompt_digest != expected_pd {
            return Err(SimonError::ReceiptMismatch(
                "prompt_digest mismatch — not this prompt".into(),
            ));
        }
        let expected_lc = token_chain_digest(&w.prompt_digest, output_token_ids)?;
        if w.output_token_chain != expected_lc {
            return Err(SimonError::ReceiptMismatch(
                "output_token_chain mismatch — not this output".into(),
            ));
        }
        Ok(())
    }

    pub fn zweryfikuj_m1(
        &self,
        tokenizer_hash: &str,
        client_nonce: &str,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
    ) -> Result<(), SimonError> {
        self.verify_m1(tokenizer_hash, client_nonce, prompt_token_ids, output_token_ids)
    }

    /// Content digest of the receipt without signature.
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        content_digest(&unsigned)
    }

    /// Signs receipt with node keypair.
    pub fn sign(mut self, keypair: &Keypair) -> Result<Self, SimonError> {
        self.signer = keypair.public();
        let digest = self.digest()?;
        self.signature = Some(keypair.sign_digest(&digest));
        Ok(self)
    }

    /// Verifies signature under signer key in receipt.
    pub fn verify_self(&self) -> Result<(), SimonError> {
        let sig = self.signature.as_ref().ok_or(SimonError::BadSignature)?;
        let digest = self.digest()?;
        self.signer.verify_digest(&digest, sig)
    }

    /// Full verification against registered node key.
    pub fn verify_for_node(
        &self,
        expected_job_id: &str,
        expected_node_id: &str,
        registered_key: &PublicKey,
    ) -> Result<(), SimonError> {
        self.verify_self()?;

        if &self.signer != registered_key {
            return Err(SimonError::UnregisteredKey);
        }

        if self.job_id != expected_job_id {
            return Err(SimonError::ReceiptMismatch(format!(
                "job_id: receipt={} expected={}",
                self.job_id, expected_job_id
            )));
        }
        if self.node_id != expected_node_id {
            return Err(SimonError::ReceiptMismatch(format!(
                "node_id: receipt={} expected={}",
                self.node_id, expected_node_id
            )));
        }
        Ok(())
    }

    #[deprecated(
        note = "wall clock difference — for logging only, not settlement; \
                use ClientObservationV1 for measured duration"
    )]
    pub fn elapsed_secs(&self) -> f64 {
        (self.finished_at_us.saturating_sub(self.started_at_us)) as f64 / 1_000_000.0
    }
}

#[cfg(test)]
mod tests_m1 {
    use super::*;
    use crate::crypto::Keypair;

    fn baseline_receipt() -> Receipt {
        Receipt {
            job_id: "job-test".into(),
            node_id: "node-test".into(),
            model_hash: "mistral-7b".into(),
            runtime: "llama.cpp/0.3".into(),
            precision: Precision::Fp16,
            activation_hash: "toploc:NOT_COMPUTED".into(),
            output_digest: output_digest_for("job-test", "hello").unwrap(),
            prompt_tokens: 3,
            completion_tokens: 2,
            started_at_us: 1,
            finished_at_us: 2,
            signer: Keypair::from_seed(&[7u8; 32]).public(),
            signature: None,
            bindings: None,
        }
    }

    #[test]
    fn legacy_receipt_verifies_signature() {
        let r: Receipt =
            serde_json::from_str(include_str!("../../../przyklady/receipt.json").trim()).unwrap();
        assert_eq!(r.level(), 0);
        assert!(r.verify_self().is_ok());
        assert!(r.matches_output(include_str!("../../../przyklady/odpowiedz.txt")));
    }

    #[test]
    fn legacy_receipt_serializes_without_m1_fields() {
        let s = serde_json::to_string(&baseline_receipt()).unwrap();
        for field in [
            "schema_version",
            "receipt_level",
            "exec_profile",
            "tokenizer_hash",
            "prompt_digest",
            "output_token_chain",
            "client_nonce",
            "sampling_params_hash",
            "rng_seed",
        ] {
            assert!(!s.contains(field), "field {field} must not serialize when None: {s}");
        }
    }

    #[test]
    fn m1_passes_and_tamper_fails() {
        let k = Keypair::from_seed(&[9u8; 32]);
        let prompt = [10u32, 20, 30];
        let out = [40u32, 50, 60];
        let r = baseline_receipt()
            .with_m1_bindings(
                "tokhash".into(),
                "nonce-1".into(),
                &prompt,
                &out,
                "params".into(),
                42,
                "llama.cpp/Q4_K_M/cpu".into(),
            )
            .unwrap()
            .sign(&k)
            .unwrap();
        assert_eq!(r.level(), 1);
        assert!(r.verify_self().is_ok());
        assert!(r.verify_m1("tokhash", "nonce-1", &prompt, &out).is_ok());
        let mut out2 = out;
        out2[1] = 999;
        assert!(r.verify_m1("tokhash", "nonce-1", &prompt, &out2).is_err());
        assert!(r.verify_m1("tokhash", "nonce-2", &prompt, &out).is_err());
        assert!(r.verify_m1("tokhash-x", "nonce-1", &prompt, &out).is_err());
    }

    #[test]
    fn verify_m1_rejects_unsigned_receipt() {
        let k = Keypair::from_seed(&[9u8; 32]);
        let prompt = [10u32, 20, 30];
        let out = [40u32, 50, 60];
        let r = baseline_receipt()
            .with_m1_bindings(
                "tokhash".into(),
                "nonce-1".into(),
                &prompt,
                &out,
                "params".into(),
                42,
                "llama.cpp/Q4_K_M/cpu".into(),
            )
            .unwrap();
        assert!(r.signature.is_none());
        assert!(r.verify_m1("tokhash", "nonce-1", &prompt, &out).is_err());
        let signed = r.sign(&k).unwrap();
        assert!(signed.verify_m1("tokhash", "nonce-1", &prompt, &out).is_ok());
    }

    #[test]
    fn sampling_commitment_is_canonical() {
        let c = SamplingCommitment::greedy();
        let d1 = c.digest().unwrap();
        let d2 = SamplingCommitment::greedy().digest().unwrap();
        assert_eq!(d1, d2);
        let mut c2 = c.clone();
        c2.top_p_x1e6 = 900_000;
        assert_ne!(d1, c2.digest().unwrap());
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains('.'), "commitment must not contain floats: {s}");
    }

    #[test]
    fn domains_are_separated() {
        assert_ne!(
            prompt_digest("t", "n", &[1, 2, 3]).unwrap(),
            token_chain_digest("t", &[1, 2, 3]).unwrap()
        );
        assert_ne!(
            params_digest(0, 1000, 1, 0, 1000).unwrap(),
            params_digest(1, 1000, 1, 0, 1000).unwrap()
        );
        assert_eq!(tokenizer_digest(b"abc"), tokenizer_digest(b"abc"));
        assert_ne!(tokenizer_digest(b"abc"), tokenizer_digest(b"abd"));
    }
}
