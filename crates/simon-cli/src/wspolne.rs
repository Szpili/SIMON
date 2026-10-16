//! Wspólne dla ról: budowa swarmu, logi, parsowanie multiaddr.

use libp2p::identity::Keypair;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{Multiaddr, SwarmBuilder};
use std::time::Duration;

/// Złożony behaviour: gossipsub (zlecenia) + request-response (prompt/wynik) + ping.
///
/// **Rozdział kanałów jest tu widoczny w typie:** prompt i wynik NIGDY nie idą
/// przez gossipsub — mają osobny protokół punkt-punkt (`simon-harness::request_response`).
#[derive(NetworkBehaviour)]
pub struct Zachowanie {
    pub gossipsub: libp2p::gossipsub::Behaviour,
    pub prompt_rr: libp2p::request_response::cbor::Behaviour<
        simon_harness::request_response::PromptRequest,
        simon_harness::request_response::PromptReply,
    >,
    pub ping: libp2p::ping::Behaviour,
    pub identify: libp2p::identify::Behaviour,
}

/// Buduje swarm z kluczem trwałym wyliczonym z seeda (albo losowym).
pub fn zbuduj_swarm(
    listen: &[String],
    bootstrap: &[String],
    // Ziarno tozsamosci SIECIOWEJ. `None` = losowy peer_id per proces
    // (patrz komentarz w node.rs). `Some(seed)` = node ma trwaly `--key`,
    // wiec i jego adres w sieci ma byc trwaly — inaczej kazdy restart
    // unieważnia adres bootstrap, ktory ktos gdzies zapisal.
    tozsamosc: Option<[u8; 32]>,
) -> Result<libp2p::Swarm<Zachowanie>, String> {
    let klucz = match tozsamosc {
        Some(seed) => klucz_p2p_z_seeda(&seed),
        None => Keypair::generate_ed25519(),
    };

    let mut swarm = SwarmBuilder::with_existing_identity(klucz)
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
        .with_behaviour(|key| {
            let gossipsub = libp2p::gossipsub::Behaviour::new(
                libp2p::gossipsub::MessageAuthenticity::Signed(key.clone()),
                libp2p::gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(1))
                    .build()
                    .expect("config gossipsub"),
            )
            .expect("gossipsub");

            Ok(Zachowanie {
                gossipsub,
                prompt_rr: simon_harness::request_response::behaviour(),
                ping: libp2p::ping::Behaviour::new(libp2p::ping::Config::new()),
                identify: libp2p::identify::Behaviour::new(
                    libp2p::identify::Config::new("simon/1.0.0".into(), key.public()),
                ),
            })
        })
        .map_err(|e| format!("behaviour: {e}"))?
        .build();

    // Nasłuch.
    for addr in listen {
        let ma: Multiaddr = addr.parse().map_err(|e| format!("zły --listen {addr}: {e}"))?;
        swarm.listen_on(ma).map_err(|e| format!("listen {addr}: {e}"))?;
    }

    // Bootstrap.
    for addr in bootstrap {
        let ma: Multiaddr = addr.parse().map_err(|e| format!("zły --bootstrap {addr}: {e}"))?;
        if let Some((peer, _)) = multiaddr_z_peerdem(&ma) {
            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
        }
        swarm
            .dial(ma)
            .map_err(|e| format!("dial {addr}: {e}"))?;
    }

    Ok(swarm)
}

/// Wyciąga `PeerId` z multiaddr zakończonego `/p2p/<id>`.
pub fn multiaddr_z_peerdem(ma: &Multiaddr) -> Option<(libp2p::PeerId, Multiaddr)> {
    let mut peer = None;
    for p in ma.iter() {
        if let libp2p::multiaddr::Protocol::P2p(id) = p {
            peer = Some(id);
        }
    }
    peer.map(|p| (p, ma.clone()))
}

/// Klucz TOZSAMOSCI SIECIOWEJ wyprowadzony z ziarna node'a.
///
/// Celowo NIE jest to ten sam klucz, ktorym node podpisuje receipty, mimo ze
/// pochodzi z tego samego ziarna. Uzywanie jednego klucza Ed25519 do dwoch
/// roznych celow (podpisy aplikacyjne + uscisk dloni transportu) to klasyczny
/// sposob na to, zeby podpis z jednego protokolu dalo sie podstawic w drugim.
/// Rozdzielamy je separacja domen.
pub fn klucz_p2p_z_seeda(seed: &[u8; 32]) -> Keypair {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"simon-p2p-identity-v1");
    h.update(seed);
    let mut bajty = [0u8; 32];
    bajty.copy_from_slice(&h.finalize());
    Keypair::ed25519_from_bytes(bajty).expect("32 bajty = poprawny klucz")
}

/// Deterministyczny klucz z seeda — tylko do testów.
#[cfg(test)]
pub fn deterministyczny_klucz(seed: u64) -> Keypair {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(seed.to_le_bytes());
    let digest = h.finalize();
    let mut bajty = [0u8; 32];
    bajty.copy_from_slice(&digest);
    Keypair::ed25519_from_bytes(bajty).expect("32 bajty = poprawny klucz")
}

/// Wypisuje adresy nasłuchu (żeby dało się podpiąć drugą maszynę).
pub fn wypisz_adresy(swarm: &libp2p::Swarm<Zachowanie>, rola: &str) {
    let peer = swarm.local_peer_id();
    println!("[simon] rola={rola} peer_id={peer}");
    for addr in swarm.listeners() {
        println!("[simon] nasłuch: {addr}");
    }
}

/// Maksymalna dopuszczalna liczba tokenów w wiązaniu M1 (ochrona przed DoS / wyczerpaniem pamięci).
pub const MAX_M1_TOKENS: usize = 65_536;

/// Wyciąga listę identyfikatorów tokenów (tablicę u32) z JSON-owej odpowiedzi `/tokenize`.
#[allow(dead_code)]
pub fn wyciagnij_id_tokenow(v: &serde_json::Value) -> Option<Vec<u32>> {
    let arr = v.get("tokens").and_then(|t| t.as_array())?;
    if arr.is_empty() || arr.len() > MAX_M1_TOKENS {
        return None;
    }
    arr.iter()
        .map(|x| x.as_u64().and_then(|n| u32::try_from(n).ok()))
        .collect::<Option<Vec<u32>>>()
}

/// Woła endpoint `/tokenize` backendu LLM i zwraca identyfikatory tokenów.
///
/// Wyprowadza bazowy URL `{baza}/tokenize` z `model_url` (obcinając `/v1/...` lub `/api/...`),
/// wysyła żądanie POST z ciałem `{"prompt": tekst, "content": tekst}` (timeout 2s)
/// i parsuje odpowiedź za pomocą `wyciagnij_id_tokenow`.
#[allow(dead_code)]
pub async fn tokenizuj_tekst(model_url: &str, tekst: &str) -> Option<Vec<u32>> {
    let trimmed = model_url.trim_end_matches('/');
    let baza = if let Some(idx) = trimmed.find("/v1") {
        &trimmed[..idx]
    } else if let Some(idx) = trimmed.find("/api") {
        &trimmed[..idx]
    } else {
        trimmed
    };
    let url = format!("{baza}/tokenize");
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(2));
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
        "prompt": tekst,
        "content": tekst,
    });
    let resp = client.post(&url).json(&payload).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let val: serde_json::Value = resp.json().await.ok()?;
    wyciagnij_id_tokenow(&val)
}

/// Oblicza domyślny odcisk tokenizera dla podanego hasza/nazwy modelu.
#[allow(dead_code)]
pub fn domyslny_odcisk_tokenizera(model_hash: &str) -> Result<String, simon_core::SimonError> {
    Ok(simon_core::receipt::odcisk_tokenizera(model_hash.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wyciagnij_id_tokenow() {
        let v1 = serde_json::json!({"tokens": [1u32, 2, 3, 4]});
        assert_eq!(wyciagnij_id_tokenow(&v1), Some(vec![1, 2, 3, 4]));

        let v2 = serde_json::json!({"tokens": []});
        assert_eq!(wyciagnij_id_tokenow(&v2), None);

        let v3 = serde_json::json!({"count": 42});
        assert_eq!(wyciagnij_id_tokenow(&v3), None);

        let v4 = serde_json::json!({"tokens": ["invalid", 42]});
        assert_eq!(wyciagnij_id_tokenow(&v4), None);

        let v_max = serde_json::json!({"tokens": vec![1u32; MAX_M1_TOKENS]});
        assert_eq!(wyciagnij_id_tokenow(&v_max), Some(vec![1u32; MAX_M1_TOKENS]));

        let v_over = serde_json::json!({"tokens": vec![1u32; MAX_M1_TOKENS + 1]});
        assert_eq!(wyciagnij_id_tokenow(&v_over), None);
    }

    #[test]
    fn test_domyslny_odcisk_tokenizera() {
        let h1 = domyslny_odcisk_tokenizera("mistral-7b").unwrap();
        let h2 = simon_core::receipt::odcisk_tokenizera(b"mistral-7b");
        assert_eq!(h1, h2);
        assert_ne!(h1, domyslny_odcisk_tokenizera("qwen-27b").unwrap());
    }

    #[test]
    fn m12_deterministyczny_klucz_jest_stabilny() {
        let a = deterministyczny_klucz(7);
        let b = deterministyczny_klucz(7);
        let c = deterministyczny_klucz(8);
        assert_eq!(a.public(), b.public(), "ten sam seed = ten sam peer");
        assert_ne!(a.public(), c.public(), "inny seed = inny peer");
    }

    #[test]
    fn tozsamosc_p2p_jest_stabilna_i_rozna_od_klucza_podpisu() {
        let seed = [7u8; 32];
        assert_eq!(klucz_p2p_z_seeda(&seed).public(), klucz_p2p_z_seeda(&seed).public(),
                   "to samo ziarno = ten sam peer_id (inaczej adres bootstrap ginie po restarcie)");
        assert_ne!(klucz_p2p_z_seeda(&seed).public(), klucz_p2p_z_seeda(&[8u8; 32]).public());

        // Klucz sieciowy NIE moze byc tym samym kluczem, co podpisujacy receipty.
        let podpisujacy = simon_core::crypto::Keypair::from_seed(&seed);
        let p2p = klucz_p2p_z_seeda(&seed);
        let p2p_bajty = match p2p.clone().try_into_ed25519() {
            Ok(k) => k.to_bytes()[32..].to_vec(),
            Err(_) => panic!("spodziewany ed25519"),
        };
        assert_ne!(p2p_bajty, podpisujacy.public().0.to_vec(),
                   "separacja domen: ten sam material klucza w dwoch rolach");
    }

    #[test]
    fn m12_kazdy_node_ma_inne_peer_id() {
        // Dwa nody z różnym seedem nie mogą udawać tego samego peera.
        let n1 = deterministyczny_klucz(1);
        let n2 = deterministyczny_klucz(2);
        assert_ne!(n1.public().to_peer_id(), n2.public().to_peer_id());
    }
}
