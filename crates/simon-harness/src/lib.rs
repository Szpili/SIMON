//! SIMON harness — client layer: dispatching work to the cluster.

pub mod acp_server;
pub mod client_protocol;
pub mod request_response;
pub mod transport;

pub use acp_server::{
    AcpServer, ActiveOrder, AuthorizationGate, OrderPhase, OrderResult, SessionId, SignatureGate,
    UserOrderProposal,
};
pub use client_protocol::{
    verify_completion, BurnProof, CoordinatorRegistry, JobOrder, OrderAccepted, OrderAuthorization,
    OrderCompleted, SpentAuthorizationRegistry, TOPIC_JOB_ORDER, TOPIC_ORDER_ACCEPTED,
    TOPIC_ORDER_COMPLETED,
};
pub use transport::{deterministic_job_id, spawn_swarm, SimonEvent, SwarmHandle};
