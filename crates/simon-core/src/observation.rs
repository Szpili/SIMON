//! Client observation — measured duration, not declared duration.

use serde::{Deserialize, Serialize};

use crate::{
    content_digest,
    crypto::{Keypair, PublicKey, Signature},
    SimonError,
};

/// Domain separator for client observations.
pub const OBSERVATION_DOMAIN: &str = "SIMON/OBSERVATION/v1";
pub const DOMENA_OBSERWACJI: &str = OBSERVATION_DOMAIN;

/// Transport used for the job request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    Libp2pRequestResponse,
}

/// Client observation measured with monotonic clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientObservationV1 {
    pub receipt_hash: String,
    pub job_id: String,
    pub client_pubkey: PublicKey,

    pub observed_time_to_first_event_ms: Option<u64>,
    pub observed_time_to_complete_ms: u64,

    pub network_complete_ms: u64,
    pub output_bound_ms: u64,
    pub execution_audit_complete_ms: Option<u64>,

    pub transport: Transport,
    pub streamed: bool,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub client_signature: Option<Signature>,
}

impl ClientObservationV1 {
    /// Content digest of observation without signature.
    pub fn digest(&self) -> Result<String, SimonError> {
        let mut unsigned = self.clone();
        unsigned.client_signature = None;
        content_digest(&serde_json::json!({
            "domena": OBSERVATION_DOMAIN,
            "obserwacja": unsigned,
        }))
    }

    pub fn sign(mut self, keypair: &Keypair) -> Result<Self, SimonError> {
        self.client_pubkey = keypair.public();
        let d = self.digest()?;
        self.client_signature = Some(keypair.sign_digest(&d));
        Ok(self)
    }

    pub fn verify_self(&self) -> Result<(), SimonError> {
        let sig = self.client_signature.as_ref().ok_or(SimonError::BadSignature)?;
        let d = self.digest()?;
        self.client_pubkey.verify_digest(&d, sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_streaming_no_ttft() {
        let obs = ClientObservationV1 {
            receipt_hash: "h".into(),
            job_id: "j".into(),
            client_pubkey: Keypair::from_seed(&[1u8; 32]).public(),
            observed_time_to_first_event_ms: None,
            observed_time_to_complete_ms: 100,
            network_complete_ms: 80,
            output_bound_ms: 20,
            execution_audit_complete_ms: None,
            transport: Transport::Libp2pRequestResponse,
            streamed: false,
            client_signature: None,
        };
        assert!(obs.observed_time_to_first_event_ms.is_none());
    }

    #[test]
    fn signed_observation_verifies() {
        let k = Keypair::from_seed(&[2u8; 32]);
        let obs = ClientObservationV1 {
            receipt_hash: "h".into(),
            job_id: "j".into(),
            client_pubkey: k.public(),
            observed_time_to_first_event_ms: None,
            observed_time_to_complete_ms: 100,
            network_complete_ms: 80,
            output_bound_ms: 20,
            execution_audit_complete_ms: None,
            transport: Transport::Libp2pRequestResponse,
            streamed: false,
            client_signature: None,
        }
        .sign(&k)
        .unwrap();

        assert!(obs.verify_self().is_ok());
    }

    #[test]
    fn modified_time_breaks_signature() {
        let k = Keypair::from_seed(&[3u8; 32]);
        let mut obs = ClientObservationV1 {
            receipt_hash: "h".into(),
            job_id: "j".into(),
            client_pubkey: k.public(),
            observed_time_to_first_event_ms: None,
            observed_time_to_complete_ms: 100,
            network_complete_ms: 80,
            output_bound_ms: 20,
            execution_audit_complete_ms: None,
            transport: Transport::Libp2pRequestResponse,
            streamed: false,
            client_signature: None,
        }
        .sign(&k)
        .unwrap();

        obs.observed_time_to_complete_ms += 1;
        assert!(obs.verify_self().is_err());
    }

    #[test]
    fn sign_sets_client_pubkey() {
        let k = Keypair::from_seed(&[4u8; 32]);
        let other = Keypair::from_seed(&[5u8; 32]);
        let obs = ClientObservationV1 {
            receipt_hash: "h".into(),
            job_id: "j".into(),
            client_pubkey: other.public(),
            observed_time_to_first_event_ms: None,
            observed_time_to_complete_ms: 100,
            network_complete_ms: 80,
            output_bound_ms: 20,
            execution_audit_complete_ms: None,
            transport: Transport::Libp2pRequestResponse,
            streamed: false,
            client_signature: None,
        }
        .sign(&k)
        .unwrap();

        assert_eq!(obs.client_pubkey, k.public());
        assert!(obs.verify_self().is_ok());
    }
}
