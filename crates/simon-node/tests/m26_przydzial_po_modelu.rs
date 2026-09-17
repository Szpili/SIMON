//! C-gate: przydział zlecenia musi filtrować po MODELU, nie tylko po mocy.
//!
//! Stan przed poprawką (sprawdzone 2026-09-17):
//!   - node NIE deklaruje, jaki ma model (`grep model` w rejestracji = 0),
//!   - `assign()` sprawdza moc/stan, ale NIE model (0 trafień),
//!   - ranking `winner()` to H(order_id ‖ coordinator_pubkey) = losowanie.
//!
//! Skutek: zlecenie na `bielik-awq` mogło trafić na node z `qwen3.8-27b`
//! i dopiero bramka D67 klienta łapała rozjazd — PO policzeniu i PO zapłacie.

use simon_core::crypto::Keypair;
use simon_node::{Capability, RegisterMsg, Registry};

fn node_z_modelem(id: &str, model: &str, max_ctx: u32) -> RegisterMsg {
    let klucz = Keypair::from_seed(&[id.len() as u8; 32]);
    RegisterMsg::new(
        id,
        klucz.public(),
        Capability {
            declared: serde_json::json!({
                "model_hash": model,
                "max_ctx": max_ctx,
            }),
            // Atestacja: w tym teście koordynator „potwierdził" deklarację.
            attested: Some(serde_json::json!({
                "model_hash": model,
                "max_ctx": max_ctx,
            })),
        },
    )
}

#[test]
fn m26_zlecenie_trafia_tylko_na_node_z_modelem() {
    let mut rejestr = Registry::new();
    rejestr.register(node_z_modelem("node-bielik", "bielik-awq", 8192));
    rejestr.register(node_z_modelem("node-qwen", "qwen3.8-27b", 36864));

    // Zlecenie prosi o bielik-awq: kandydaci MUSZĄ być odfiltrowani PO MODELU.
    let kandydaci = rejestr.kandydaci_dla_modelu("bielik-awq", 0);
    assert_eq!(kandydaci, vec!["node-bielik".to_string()],
        "zlecenie na bielik-awq NIE MOŻE trafić na node z qwen");

    // Zlecenie prosi o qwen: tylko qwen.
    let kandydaci = rejestr.kandydaci_dla_modelu("qwen3.8-27b", 0);
    assert_eq!(kandydaci, vec!["node-qwen".to_string()]);

    // Model, którego nikt nie ma: pusta lista = jawna odmowa, nie cichy wybór.
    let kandydaci = rejestr.kandydaci_dla_modelu("llama-70b", 0);
    assert!(kandydaci.is_empty(), "brak node'a z modelem = pusta lista, nie losowanie");
}

#[test]
fn m26_zlecenie_trafia_tylko_gdzie_zmiesci_sie_kontekst() {
    let mut rejestr = Registry::new();
    rejestr.register(node_z_modelem("node-maly", "bielik-awq", 8192));
    rejestr.register(node_z_modelem("node-duzy", "bielik-awq", 36864));

    // Zlecenie 20k tokenów: node z 8k limitu ODPADA.
    let kandydaci = rejestr.kandydaci_dla_modelu("bielik-awq", 20_000);
    assert_eq!(kandydaci, vec!["node-duzy".to_string()],
        "zlecenie 20k NIE MOŻE trafić na node z limitem 8k");

    // Zlecenie 5k: oba pasują.
    let kandydaci = rejestr.kandydaci_dla_modelu("bielik-awq", 5_000);
    assert_eq!(kandydaci.len(), 2, "w limicie obu node'ów");
}
