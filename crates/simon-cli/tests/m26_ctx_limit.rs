//! C (M2.6-limit) — bramka limitu kontekstu po stronie node'a.
//!
//! Teza: zlecenie żądające więcej kontekstu, niż node ma, ma zostać
//! odrzucone KONTROLOWANYM błędem (`ctx_ponad_limit`), a nie wywalić się
//! błędem z vLLM.
//!
//! Ten test padał PRZED poprawką — nie było ani pola `max_ctx`, ani bramki.

use simon_harness::request_response::{PromptError, PromptReply, PromptRequest};

/// Kopia bramki z node.rs — sprawdzamy regułę, nie I/O.
fn bramka(request: &PromptRequest, max_ctx: u32) -> Option<PromptReply> {
    if max_ctx > 0 && request.max_ctx > max_ctx {
        Some(PromptReply::Blad(PromptError {
            v: 1,
            order_id: request.order_id.clone(),
            kod: "ctx_ponad_limit".to_string(),
            opis: format!(
                "zlecenie żąda {} tokenów kontekstu, ten node ma limit {}",
                request.max_ctx, max_ctx
            ),
            realny_ctx: Some(request.max_ctx),
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
    }
}

#[test]
fn m26_zlecenie_ponad_limit_odrzucone() {
    // Bielik na Franko ma 8192. Zlecenie żąda 32000.
    match bramka(&req(32_000), 8_192) {
        Some(PromptReply::Blad(e)) => {
            assert_eq!(e.kod, "ctx_ponad_limit", "kontrolowany kod błędu");
        }
        _ => panic!("zlecenie ponad limit MUSI być odrzucone"),
    }
}

#[test]
fn m26_zlecenie_w_limicie_przechodzi() {
    assert!(bramka(&req(5_000), 8_192).is_none(), "w limicie = brak odrzucenia");
}

#[test]
fn m26_dokladnie_na_limicie_przechodzi() {
    assert!(bramka(&req(8_192), 8_192).is_none(), "równo z limitem = OK");
}

#[test]
fn m26_brak_znanego_limitu_nie_blokuje() {
    // Gdy node nie odczytał max_model_len (max_ctx=0), NIE blokujemy —
    // lepiej spróbować i dostać błąd vLLM, niż blokować wszystko.
    assert!(bramka(&req(999_999), 0).is_none(), "nieznany limit = przepuszczamy");
}
