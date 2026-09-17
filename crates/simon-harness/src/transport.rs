//! Warstwa transportu: swarm libp2p + gossipsub (decyzja Karola 2026-09-16).
//!
//! „Ma być jak torrent inference" — klient ROZGŁASZA zlecenie do swarmu,
//! nie pyta konkretnego serwera. Discovery przez Kademlię (DHT), bez trackera.
//!
//! Ten moduł to WYŁĄCZNIE transport i cykl życia swarmu. Cała logika protokołu
//! (walidacja, wyścig, weryfikacja receiptu) siedzi w `client_protocol` — tutaj
//! jest tylko przewiezienie bajtów.
//!
//! UWAGA (karpathy): nie dodajemy tu logiki. Transport ma przewozić, nie decydować.


use std::time::Duration;

use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity};
use libp2p::kad;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identify, noise, ping, yamux, Multiaddr, PeerId, SwarmBuilder};
use tokio::sync::mpsc;

/// Tematy gossipsub jako stałe — część nieodwracalnego kontraktu sieci.
pub const TOPIC_JOB_ORDER: &str = "simon/v1/job-order";
pub const TOPIC_ORDER_ACCEPTED: &str = "simon/v1/order-accepted";
pub const TOPIC_ORDER_COMPLETED: &str = "simon/v1/order-completed";

/// Zachowanie swarmu: gossipsub + Kademlia (discovery) + identify + ping.
///
/// `ping` daje RTT do D6 (optymalizacja po pingu). `identify` pozwala poznać
/// adresy peerów, `kad` rozgłasza i szuka ich bez serwera centralnego.
#[derive(NetworkBehaviour)]
pub struct SimonBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
}

/// Zdarzenie wyjęte z swarmu — to, co interesuje warstwę wyżej.
#[derive(Debug)]
pub enum SimonEvent {
    /// Przyszedł bajt z tematu. Dekodowanie robi warstwa wyżej.
    Message { topic: String, data: Vec<u8>, from: PeerId },
    /// Nowy peer dołączył do swarmu.
    PeerConnected(PeerId),
    /// Peer wyszedł.
    PeerDisconnected(PeerId),
    /// Znamy nowy adres (discovery).
    AddressDiscovered(PeerId, Multiaddr),
    /// Swarm zaczął nasłuchiwać na tym adresie (potrzebne do dialowania w testach
    /// i do bootstrapu — bez tego nie wiemy, pod jakim portem realnie słuchamy).
    Listening(Multiaddr),
}

/// Buduje zachowanie swarmu z domyślnymi parametrami gossipsub.
pub fn build_behaviour(keypair: &libp2p::identity::Keypair) -> Result<SimonBehaviour, String> {
    // gossipsub: publikujemy podpisywane wiadomości (transport sam w sobie
    // nie jest zaufany — RULES #6). Duże wiadomości (receipt z aktywacjami)
    // nie mieszczą się w domyślnym limicie, więc podnosimy go.
    let config = gossipsub::ConfigBuilder::default()
        .max_transmit_size(4 * 1024 * 1024) // 4 MB — receipt z TopLoc + output
        .validation_mode(gossipsub::ValidationMode::Strict)
        .heartbeat_interval(Duration::from_secs(1))
        .build()
        .map_err(|e| format!("gossipsub config: {e}"))?;

    let gossipsub = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(keypair.clone()),
        config,
    )
    .map_err(|e| format!("gossipsub behaviour: {e}"))?;

    let peer_id = keypair.public().to_peer_id();
    let kad = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));

    let identify = identify::Behaviour::new(identify::Config::new(
        "simon/v1".to_string(),
        keypair.public(),
    ));

    let ping = ping::Behaviour::default();

    Ok(SimonBehaviour {
        gossipsub,
        kad,
        identify,
        ping,
    })
}

/// Uruchamia pętlę swarmu. Zwraca kanał do wysyłania poleceń.
///
/// `listen` — adresy nasłuchu (np. `/ip4/0.0.0.0/tcp/0` = dowolny port).
/// Polecenia do swarmu idą przez kanał (Swarm nie jest Send-friendly wprost).
pub struct SwarmHandle {
    pub peer_id: PeerId,
    cmd_tx: mpsc::UnboundedSender<SwarmCommand>,
}

/// Polecenia, które warstwa wyżej może wysłać do swarmu.
#[derive(Debug)]
pub enum SwarmCommand {
    /// Rozgłoś bajty na temat.
    Publish { topic: String, data: Vec<u8> },
    /// Subskrybuj temat.
    Subscribe(String),
    /// Połącz się z konkretnym adresem (do testów i bootstrapu).
    Dial(Multiaddr),
    /// Dodaj adres do tablicy routingu Kademlii.
    AddAddress(PeerId, Multiaddr),
    /// Zamknij pętlę.
    Shutdown,
}

impl SwarmHandle {
    pub fn publish(&self, topic: &str, data: Vec<u8>) -> Result<(), String> {
        self.cmd_tx
            .send(SwarmCommand::Publish {
                topic: topic.to_string(),
                data,
            })
            .map_err(|e| format!("swarm zamknięty: {e}"))
    }

    pub fn subscribe(&self, topic: &str) -> Result<(), String> {
        self.cmd_tx
            .send(SwarmCommand::Subscribe(topic.to_string()))
            .map_err(|e| format!("swarm zamknięty: {e}"))
    }

    pub fn dial(&self, addr: Multiaddr) -> Result<(), String> {
        self.cmd_tx
            .send(SwarmCommand::Dial(addr))
            .map_err(|e| format!("swarm zamknięty: {e}"))
    }

    pub fn add_address(&self, peer: PeerId, addr: Multiaddr) -> Result<(), String> {
        self.cmd_tx
            .send(SwarmCommand::AddAddress(peer, addr))
            .map_err(|e| format!("swarm zamknięty: {e}"))
    }

    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(SwarmCommand::Shutdown);
    }
}

/// Tworzy swarm i zwraca (uchwyt, strumień zdarzeń).
///
/// To jest jedyna funkcja, która realnie dotyka sieci. Cała reszta protokołu
/// jest testowalna bez niej (patrz `tests/cykl_zlecenie.rs`).
pub async fn spawn_swarm(
    listen: Vec<Multiaddr>,
) -> Result<(SwarmHandle, mpsc::UnboundedReceiver<SimonEvent>), String> {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();
    let behaviour = build_behaviour(&keypair)?;

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|e| format!("tcp: {e}"))?
        .with_quic()
        .with_behaviour(|_| behaviour)
        .map_err(|e| format!("behaviour: {e}"))?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    for addr in listen {
        swarm
            .listen_on(addr)
            .map_err(|e| format!("listen: {e}"))?;
    }

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<SwarmCommand>();
    let (evt_tx, evt_rx) = mpsc::unbounded_channel::<SimonEvent>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(SwarmCommand::Publish { topic, data }) => {
                            let t = IdentTopic::new(&topic);
                            if let Err(e) = swarm.behaviour_mut().gossipsub.publish(t, data) {
                                eprintln!("[swarm] publish {topic}: {e}");
                            }
                        }
                        Some(SwarmCommand::Subscribe(topic)) => {
                            let t = IdentTopic::new(&topic);
                            if let Err(e) = swarm.behaviour_mut().gossipsub.subscribe(&t) {
                                eprintln!("[swarm] subscribe {topic}: {e}");
                            }
                        }
                        Some(SwarmCommand::Dial(addr)) => {
                            if let Err(e) = swarm.dial(addr) {
                                eprintln!("[swarm] dial: {e}");
                            }
                        }
                        Some(SwarmCommand::AddAddress(peer, addr)) => {
                            swarm.behaviour_mut().kad.add_address(&peer, addr);
                        }
                        Some(SwarmCommand::Shutdown) | None => break,
                    }
                }
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(SimonBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message { message, .. }
                        )) => {
                            let _ = evt_tx.send(SimonEvent::Message {
                                topic: message.topic.to_string(),
                                data: message.data,
                                from: message.source.unwrap_or(PeerId::random()),
                            });
                        }
                        SwarmEvent::Behaviour(SimonBehaviourEvent::Identify(
                            identify::Event::Received { peer_id, info, .. }
                        )) => {
                            for addr in info.listen_addrs {
                                swarm.behaviour_mut().kad.add_address(&peer_id, addr.clone());
                                let _ = evt_tx.send(SimonEvent::AddressDiscovered(peer_id, addr));
                            }
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            let _ = evt_tx.send(SimonEvent::Listening(address));
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            let _ = evt_tx.send(SimonEvent::PeerConnected(peer_id));
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            let _ = evt_tx.send(SimonEvent::PeerDisconnected(peer_id));
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    Ok((SwarmHandle { peer_id, cmd_tx }, evt_rx))
}

/// Deterministyczny identyfikator zlecenia dla rozstrzygania wyścigu.
///
/// Przy gossipsub wielu koordynatorów widzi to samo zlecenie. Ta funkcja daje
/// im wszystkim ten sam `job_id` z `order_id` — bez uzgadniania.
/// (Właściwy przydział robi VRF; to tylko identyfikator.)
pub fn deterministic_job_id(order_id: &str) -> String {
    // SHA-256, NIE DefaultHasher. DefaultHasher nie ma gwarantowanego algorytmu
    // między wersjami Rusta — Szpon i Frank na różnych rustc policzyłyby różne
    // job_id dla tego samego zlecenia. W jednym procesie (testy) tego nie widać.
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(order_id.as_bytes());
    format!("job-{}", hex::encode(h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_id_jest_deterministyczny() {
        let a = deterministic_job_id("o-1");
        let b = deterministic_job_id("o-1");
        assert!(a.starts_with("job-"), "prefiks job-");
        assert_eq!(a.len(), 4 + 64, "sha256 hex = 64 znaki");
        assert_eq!(a, b, "ten sam order_id musi dać ten sam job_id");
        assert_ne!(a, deterministic_job_id("o-2"));
    }

    #[test]
    fn tematy_transportu_zgodne_z_kontraktem() {
        assert_eq!(TOPIC_JOB_ORDER, "simon/v1/job-order");
        assert_eq!(TOPIC_ORDER_ACCEPTED, "simon/v1/order-accepted");
        assert_eq!(TOPIC_ORDER_COMPLETED, "simon/v1/order-completed");
    }

    #[test]
    fn behaviour_buduje_sie_z_poprawnym_kluczem() {
        let kp = libp2p::identity::Keypair::generate_ed25519();
        assert!(build_behaviour(&kp).is_ok());
    }
}
