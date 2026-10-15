//! SIMON harness — warstwa klienta: zlecanie pracy do klastra.
//!
//! To jest miejsce, gdzie harness gada z koordynatorem. Protokół node↔koordynator
//! istnieje w `~/DC`; TU jest brakująca trzecia warstwa: **klient → koordynator**.
//!
//! Decyzje, które realizuje:
//!   - D83  klient PŁACI, a opłata jest SPALONA → kopanie z własnych promptów
//!     nie opłaca się (spalasz tyle, ile zarabiasz)
//!   - D84  zadanie przydziela LOSOWANIE (VRF) — klient nie wybiera node'a
//!   - D11  użytkownik wybiera MODEL, nie maszyny

pub mod acp_server;
pub mod request_response;
pub mod client_protocol;
pub mod transport;

pub use client_protocol::{
    verify_completion, BurnProof, JobOrder, OrderAccepted, OrderCompleted, TOPIC_JOB_ORDER,
    TOPIC_ORDER_ACCEPTED, TOPIC_ORDER_COMPLETED,
};
pub use transport::{deterministic_job_id, spawn_swarm, SimonEvent, SwarmHandle};
pub use acp_server::{
    AcpServer, BramkaAutoryzacji, BramkaPodpisu, FazaZlecenia, SessionId, WynikZlecenia,
    ZlecenieUzytkownika,
};
