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
    seed: Option<u64>,
) -> Result<libp2p::Swarm<Zachowanie>, String> {
    let klucz = match seed {
        Some(s) => deterministyczny_klucz(s),
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

/// Deterministyczny klucz z seeda — do testów i stabilnych tożsamości.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m12_deterministyczny_klucz_jest_stabilny() {
        let a = deterministyczny_klucz(7);
        let b = deterministyczny_klucz(7);
        let c = deterministyczny_klucz(8);
        assert_eq!(a.public(), b.public(), "ten sam seed = ten sam peer");
        assert_ne!(a.public(), c.public(), "inny seed = inny peer");
    }

    #[test]
    fn m12_kazdy_node_ma_inne_peer_id() {
        // Dwa nody z różnym seedem nie mogą udawać tego samego peera.
        let n1 = deterministyczny_klucz(1);
        let n2 = deterministyczny_klucz(2);
        assert_ne!(n1.public().to_peer_id(), n2.public().to_peer_id());
    }
}
