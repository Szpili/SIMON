//! C (M2.6-limit) - context limit gate on node side.
//!
//! Orders requesting more context than the node supports are rejected with a controlled
//! error code (`ctx_ponad_limit`) rather than failing ungracefully inside vLLM.

use simon_harness::request_response::{PromptError, PromptReply, PromptRequest};

/// Copy of gate logic from node.rs - tests rule, not I/O.
fn bramka(request: &PromptRequest, max_ctx: u32) -> Option<PromptReply> {
    if max_ctx > 0 && request.max_ctx > max_ctx {
        Some(PromptReply::Error(PromptError {
            v: 1,
            order_id: request.order_id.clone(),
            code: "ctx_ponad_limit".to_string(),
            description: format!(
                "order requested {} context tokens, node limit is {}",
                request.max_ctx, max_ctx
            ),
            actual_ctx: Some(request.max_ctx),
            limit_ctx: Some(max_ctx),
        }))
    } else {
        None
    }
}

fn req(ctx: u32) -> PromptRequest {
    PromptRequest {
        v: 1,
        order_id: "o1".into(),
        job_id: "j1".into(),
        model_hash: "bielik-awq".into(),
        prompt: "test".into(),
        max_tokens: 128,
        max_ctx: ctx,
        nonce: "n1".into(),
        chat: false,
    }
}

#[test]
fn m26_zlecenie_ponad_limit_odrzucone() {
    match bramka(&req(32_000), 8_192) {
        Some(PromptReply::Error(e)) => {
            assert_eq!(e.code, "ctx_ponad_limit", "controlled error code");
        }
        _ => panic!("order exceeding limit must be rejected"),
    }
}

#[test]
fn m26_zlecenie_w_limicie_przechodzi() {
    assert!(bramka(&req(5_000), 8_192).is_none(), "within limit = no rejection");
}

#[test]
fn m26_dokladnie_na_limicie_przechodzi() {
    assert!(bramka(&req(8_192), 8_192).is_none(), "exactly on limit = OK");
}

#[test]
fn m26_brak_znanego_limitu_nie_blokuje() {
    // When node has no max_model_len (max_ctx=0), we do not block preemptively.
    assert!(bramka(&req(999_999), 0).is_none(), "unknown limit = pass through");
}
