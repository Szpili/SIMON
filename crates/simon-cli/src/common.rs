//! Common utilities across CLI roles: swarm setup, logging, multiaddr parsing, and tokenization.

use libp2p::identity::Keypair;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{Multiaddr, SwarmBuilder};
use std::time::Duration;

/// Combined swarm behaviour: gossipsub (orders) + request-response (prompt/result) + ping + identify.
#[derive(NetworkBehaviour)]
pub struct SwarmBehaviour {
    pub gossipsub: libp2p::gossipsub::Behaviour,
    pub prompt_rr: libp2p::request_response::cbor::Behaviour<
        simon_harness::request_response::PromptRequest,
        simon_harness::request_response::PromptReply,
    >,
    pub ping: libp2p::ping::Behaviour,
    pub identify: libp2p::identify::Behaviour,
}

/// Builds a libp2p swarm with a persistent identity key from seed (or random).
pub fn build_swarm(
    listen: &[String],
    bootstrap: &[String],
    identity_seed: Option<[u8; 32]>,
) -> Result<libp2p::Swarm<SwarmBehaviour>, String> {
    let key = match identity_seed {
        Some(seed) => p2p_key_from_seed(&seed),
        None => Keypair::generate_ed25519(),
    };

    let mut swarm = SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| format!("tcp: {e}"))?
        .with_quic()
        .with_dns()
        .map_err(|e| format!("dns: {e}"))?
        .with_behaviour(|k| {
            let gossipsub = libp2p::gossipsub::Behaviour::new(
                libp2p::gossipsub::MessageAuthenticity::Signed(k.clone()),
                libp2p::gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(1))
                    .build()
                    .expect("config gossipsub"),
            )
            .expect("gossipsub");

            Ok(SwarmBehaviour {
                gossipsub,
                prompt_rr: simon_harness::request_response::behaviour(),
                ping: libp2p::ping::Behaviour::new(libp2p::ping::Config::new()),
                identify: libp2p::identify::Behaviour::new(
                    libp2p::identify::Config::new("simon/1.0.0".into(), k.public()),
                ),
            })
        })
        .map_err(|e| format!("behaviour: {e}"))?
        .build();

    for addr in listen {
        let ma: Multiaddr = addr.parse().map_err(|e| format!("invalid --listen {addr}: {e}"))?;
        swarm.listen_on(ma).map_err(|e| format!("listen {addr}: {e}"))?;
    }

    for addr in bootstrap {
        let ma: Multiaddr = addr.parse().map_err(|e| format!("invalid --bootstrap {addr}: {e}"))?;
        if let Some((peer, _)) = multiaddr_with_peer(&ma) {
            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
        }
        swarm
            .dial(ma)
            .map_err(|e| format!("dial {addr}: {e}"))?;
    }

    Ok(swarm)
}

/// Extracts `PeerId` from multiaddr ending with `/p2p/<id>`.
pub fn multiaddr_with_peer(ma: &Multiaddr) -> Option<(libp2p::PeerId, Multiaddr)> {
    let mut peer = None;
    for p in ma.iter() {
        if let libp2p::multiaddr::Protocol::P2p(id) = p {
            peer = Some(id);
        }
    }
    peer.map(|p| (p, ma.clone()))
}

/// Network identity keypair derived from node seed (with domain separation).
pub fn p2p_key_from_seed(seed: &[u8; 32]) -> Keypair {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"simon-p2p-identity-v1");
    h.update(seed);
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&h.finalize());
    Keypair::ed25519_from_bytes(bytes).expect("32 bytes = valid key")
}

/// Deterministic key from seed (tests only).
#[cfg(test)]
pub fn deterministic_key(seed: u64) -> Keypair {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(seed.to_le_bytes());
    let digest = h.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&digest);
    Keypair::ed25519_from_bytes(bytes).expect("32 bytes = valid key")
}

/// Prints listen addresses for discovery.
pub fn print_listen_addrs(swarm: &libp2p::Swarm<SwarmBehaviour>, role: &str) {
    let peer = swarm.local_peer_id();
    println!("[simon] role={role} peer_id={peer}");
    for addr in swarm.listeners() {
        println!("[simon] listen: {addr}");
    }
}

/// Maximum allowed tokens in M1 binding (DoS / memory exhaustion guard).
pub const MAX_M1_TOKENS: usize = 65_536;

/// Extracts token ID array (u32) from `/tokenize` JSON response.
pub fn extract_token_ids(v: &serde_json::Value) -> Option<Vec<u32>> {
    let arr = v.get("tokens").and_then(|t| t.as_array())?;
    if arr.is_empty() || arr.len() > MAX_M1_TOKENS {
        return None;
    }
    arr.iter()
        .map(|x| x.as_u64().and_then(|n| u32::try_from(n).ok()))
        .collect::<Option<Vec<u32>>>()
}

/// Calls `/tokenize` endpoint of LLM backend and returns token IDs.
pub async fn tokenize_text(model_url: &str, text: &str) -> Option<Vec<u32>> {
    let trimmed = model_url.trim_end_matches('/');
    let base = if let Some(idx) = trimmed.find("/v1") {
        &trimmed[..idx]
    } else if let Some(idx) = trimmed.find("/api") {
        &trimmed[..idx]
    } else {
        trimmed
    };
    let url = format!("{base}/tokenize");
    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(2));
    if let Ok(k) = std::env::var("SIMON_MODEL_API_KEY") {
        if !k.is_empty() {
            let mut h = reqwest::header::HeaderMap::new();
            if let Ok(mut v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {k}")) {
                v.set_sensitive(true);
                h.insert(reqwest::header::AUTHORIZATION, v);
            }
            builder = builder.default_headers(h);
        }
    }
    let client = builder.build().ok()?;
    let payload = serde_json::json!({
        "prompt": text,
        "content": text,
    });
    let resp = client.post(&url).json(&payload).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let val: serde_json::Value = resp.json().await.ok()?;
    extract_token_ids(&val)
}

/// Computes default tokenizer digest for a given model hash/name.
pub fn default_tokenizer_digest(model_hash: &str) -> Result<String, simon_core::SimonError> {
    Ok(simon_core::receipt::tokenizer_digest(model_hash.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_token_ids() {
        let v1 = serde_json::json!({"tokens": [1u32, 2, 3, 4]});
        assert_eq!(extract_token_ids(&v1), Some(vec![1, 2, 3, 4]));

        let v2 = serde_json::json!({"tokens": []});
        assert_eq!(extract_token_ids(&v2), None);

        let v3 = serde_json::json!({"count": 42});
        assert_eq!(extract_token_ids(&v3), None);

        let v4 = serde_json::json!({"tokens": ["invalid", 42]});
        assert_eq!(extract_token_ids(&v4), None);

        let v_max = serde_json::json!({"tokens": vec![1u32; MAX_M1_TOKENS]});
        assert_eq!(extract_token_ids(&v_max), Some(vec![1u32; MAX_M1_TOKENS]));

        let v_over = serde_json::json!({"tokens": vec![1u32; MAX_M1_TOKENS + 1]});
        assert_eq!(extract_token_ids(&v_over), None);
    }

    #[test]
    fn test_default_tokenizer_digest() {
        let h1 = default_tokenizer_digest("mistral-7b").unwrap();
        let h2 = simon_core::receipt::tokenizer_digest(b"mistral-7b");
        assert_eq!(h1, h2);
        assert_ne!(h1, default_tokenizer_digest("qwen-27b").unwrap());
    }

    #[test]
    fn m12_deterministic_key_is_stable() {
        let a = deterministic_key(7);
        let b = deterministic_key(7);
        let c = deterministic_key(8);
        assert_eq!(a.public(), b.public(), "same seed = same peer");
        assert_ne!(a.public(), c.public(), "different seed = different peer");
    }

    #[test]
    fn p2p_identity_is_stable_and_distinct_from_signing_key() {
        let seed = [7u8; 32];
        assert_eq!(p2p_key_from_seed(&seed).public(), p2p_key_from_seed(&seed).public(),
                   "same seed = same peer_id");
        assert_ne!(p2p_key_from_seed(&seed).public(), p2p_key_from_seed(&[8u8; 32]).public());

        let signer = simon_core::crypto::Keypair::from_seed(&seed);
        let p2p = p2p_key_from_seed(&seed);
        let p2p_bytes = match p2p.clone().try_into_ed25519() {
            Ok(k) => k.to_bytes()[32..].to_vec(),
            Err(_) => panic!("expected ed25519"),
        };
        assert_ne!(p2p_bytes, signer.public().0.to_vec(),
                   "domain separation: same key material in two roles");
    }

    #[test]
    fn m12_each_node_has_distinct_peer_id() {
        let n1 = deterministic_key(1);
        let n2 = deterministic_key(2);
        assert_ne!(n1.public().to_peer_id(), n2.public().to_peer_id());
    }
}
