//! M1.1 — request-response channel for prompt and output (NEVER gossipsub).

use libp2p::request_response::{self, ProtocolSupport};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Request for prompt computation (client -> node).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptRequest {
    /// Format version.
    pub v: u16,
    pub order_id: String,
    pub job_id: String,
    /// Requested model (D11: user selects MODEL, not node).
    pub model_hash: String,
    /// Prompt content — via point-to-point channel, never gossipsub.
    pub prompt: String,
    pub max_tokens: u32,
    /// Context tokens declared by client.
    pub max_ctx: u32,
    /// Order nonce (M0.2/M0.4b) — binds prompt to authorization.
    pub nonce: String,
    /// Chat mode flag (structured JSON messages and tool calls).
    #[serde(default)]
    pub chat: bool,
}

/// Node response (node -> client).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptResponse {
    pub v: u16,
    pub order_id: String,
    pub job_id: String,
    /// Inference output text. Sent together with receipt — client verifies locally.
    pub output: String,
    /// Signed receipt JSON.
    pub receipt: String,
    pub tokens_out: u32,
    pub ttft_ms: u64,
    pub gen_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokenizer_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_token_ids: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_token_ids: Option<Vec<u32>>,
}

/// Error returned by node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptError {
    pub v: u16,
    pub order_id: String,
    #[serde(rename = "code", alias = "kod")]
    pub code: String,
    #[serde(rename = "description", alias = "opis")]
    pub description: String,
    #[serde(rename = "actual_ctx", alias = "realny_ctx")]
    pub actual_ctx: Option<u32>,
    pub limit_ctx: Option<u32>,
}

/// Reply wrapper — either success or error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PromptReply {
    Ok(PromptResponse),
    #[serde(rename = "Error", alias = "Blad")]
    Error(PromptError),
}

/// Protocols for request-response (M1.1).
#[derive(Debug, Clone)]
pub struct PromptProtocol;

impl AsRef<str> for PromptProtocol {
    fn as_ref(&self) -> &str {
        "/simon/prompt/1"
    }
}

pub type ProtokolPromptu = PromptProtocol;

#[derive(Debug, Clone)]
pub struct ResultProtocol;

impl AsRef<str> for ResultProtocol {
    fn as_ref(&self) -> &str {
        "/simon/result/1"
    }
}

pub type ProtokolWyniku = ResultProtocol;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// Builds request-response behaviour for prompts (client <-> node).
pub fn behaviour() -> request_response::cbor::Behaviour<PromptRequest, PromptReply> {
    request_response::cbor::Behaviour::new(
        [(
            libp2p::StreamProtocol::new(PromptProtocol.as_ref()),
            ProtocolSupport::Full,
        )],
        request_response::Config::default().with_request_timeout(REQUEST_TIMEOUT),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m11_prompt_is_not_in_gossipsub() {
        assert!(PromptProtocol.as_ref().starts_with("/simon/prompt/"));
        assert!(!PromptProtocol.as_ref().contains("gossipsub"));
        assert_ne!(PromptProtocol.as_ref(), ResultProtocol.as_ref());
    }

    #[test]
    fn m11_prompt_roundtrip() {
        let req = PromptRequest {
            v: 1,
            order_id: "abc".into(),
            job_id: "job1".into(),
            model_hash: "sha256:qwen".into(),
            prompt: "compute this".into(),
            max_tokens: 128,
            max_ctx: 1000,
            nonce: "n1".into(),
            chat: false,
        };
        let bytes = serde_json::to_vec(&req).unwrap();
        let back: PromptRequest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn m11_result_roundtrip() {
        let r = PromptReply::Ok(PromptResponse {
            v: 1,
            order_id: "abc".into(),
            job_id: "job1".into(),
            output: "result".into(),
            receipt: "{}".into(),
            tokens_out: 5,
            ttft_ms: 1740,
            gen_ms: 230,
            tokenizer_hash: None,
            prompt_token_ids: None,
            output_token_ids: None,
        });
        let back: PromptReply = serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
        assert_eq!(r, back);
    }

    #[tokio::test]
    async fn m11_cbor_roundtrip_with_m1_tokens() {
        use libp2p::request_response::Codec;
        let mut codec = libp2p::request_response::cbor::codec::Codec::<PromptRequest, PromptReply>::default();
        let protocol = libp2p::StreamProtocol::new(PromptProtocol.as_ref());
        let reply = PromptReply::Ok(PromptResponse {
            v: 1,
            order_id: "order-123".into(),
            job_id: "job-123".into(),
            output: "test output".into(),
            receipt: "{}".into(),
            tokens_out: 42,
            ttft_ms: 120,
            gen_ms: 450,
            tokenizer_hash: Some("sha256:tok_hash".into()),
            prompt_token_ids: Some(vec![101, 102, 103]),
            output_token_ids: Some(vec![201, 202, 203, 204]),
        });
        let mut buf = Vec::new();
        codec.write_response(&protocol, &mut buf, reply.clone()).await.unwrap();
        let mut slice = &buf[..];
        let decoded: PromptReply = codec.read_response(&protocol, &mut slice).await.unwrap();
        assert_eq!(reply, decoded);
    }
}
