//! C-gate: order routing must filter by MODEL, not just compute capacity.
//!
//! Pre-fix state:
//!   - node did not declare model,
//!   - `assign()` verified capacity/state, but not model,
//!   - ranking `winner()` was random H(order_id || coordinator_pubkey).
//!
//! Consequence: job for `bielik-awq` could route to node with `qwen3.8-27b`,
//! caught only downstream at client D67 verification after payment.

use simon_core::crypto::Keypair;
use simon_node::{Capability, RegisterMsg, Registry};

fn node_with_model(id: &str, model: &str, max_ctx: u32) -> RegisterMsg {
    let key = Keypair::from_seed(&[id.len() as u8; 32]);
    RegisterMsg::new(
        id,
        key.public(),
        Capability {
            declared: serde_json::json!({
                "model_hash": model,
                "max_ctx": max_ctx,
            }),
            // Attestation: coordinator attested the declaration in this test.
            attested: Some(serde_json::json!({
                "model_hash": model,
                "max_ctx": max_ctx,
            })),
        },
    )
}

#[test]
fn m26_order_routes_only_to_node_with_matching_model() {
    let mut registry = Registry::new();
    registry.register(node_with_model("node-bielik", "bielik-awq", 8192));
    registry.register(node_with_model("node-qwen", "qwen3.8-27b", 36864));

    // Order asks for bielik-awq: candidates MUST be filtered BY MODEL.
    let candidates = registry.candidates_for_model("bielik-awq", 0);
    assert_eq!(
        candidates,
        vec!["node-bielik".to_string()],
        "job for bielik-awq MUST NOT route to node with qwen"
    );

    // Order asks for qwen: only qwen.
    let candidates = registry.candidates_for_model("qwen3.8-27b", 0);
    assert_eq!(candidates, vec!["node-qwen".to_string()]);

    // Model no node hosts: empty list = explicit refusal, not random choice.
    let candidates = registry.candidates_for_model("llama-70b", 0);
    assert!(
        candidates.is_empty(),
        "missing node with model = empty list, not random assignment"
    );
}

#[test]
fn m26_order_routes_only_where_context_fits() {
    let mut registry = Registry::new();
    registry.register(node_with_model("node-small", "bielik-awq", 8192));
    registry.register(node_with_model("node-large", "bielik-awq", 36864));

    // 20k token job: 8k limit node is dropped.
    let candidates = registry.candidates_for_model("bielik-awq", 20_000);
    assert_eq!(
        candidates,
        vec!["node-large".to_string()],
        "20k order MUST NOT route to node with 8k limit"
    );

    // 5k token job: both fit.
    let candidates = registry.candidates_for_model("bielik-awq", 5_000);
    assert_eq!(candidates.len(), 2, "within limits of both nodes");
}
