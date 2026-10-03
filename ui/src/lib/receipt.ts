//! Kształt receiptu — zgodny 1:1 z `simon-core::receipt::Receipt`.
//!
//! Świadomie NIE ma tu logiki kryptograficznej. Weryfikacja żyje w Rust
//! (`simon-core`, eksport WASM w `./wasm/`); ten plik tylko opisuje dane.

/** M1: wiązanie wejścia/stanu zapisane w receipcie (opcjonalne). */
export interface WiazaniaM1 {
  schema_version: number;
  receipt_level: number;
  exec_profile: string;
  tokenizer_hash: string;
  prompt_digest: string;
  output_token_chain: string;
  client_nonce: string;
  sampling_params_hash: string;
  rng_seed: number;
}

export interface Receipt {
  job_id: string;
  node_id: string;
  /** Hash modelu (D67): niezgodny = inny klaster. */
  model_hash: string;
  /** Wersja runtime (D67): niezgodna = nie wchodzi do puli. */
  runtime: string;
  precision: 'bf16' | 'fp16' | 'fp32' | 'fp8' | 'fp4' | string;
  /** LSH aktywacji wg TopLoc (na dziś zwykle "NIE_POLICZONY"). */
  activation_hash: string;
  /** Odcisk TREŚCI wyniku, domenowo rozdzielony i związany z job_id. */
  output_digest: string;
  prompt_tokens: number;
  completion_tokens: number;
  started_at_us: number;
  finished_at_us: number;
  /** Klucz publiczny node'a (hex, 32 B) — część podpisywanej treści. */
  signer: string;
  /** Podpis Ed25519 (hex, 64 B) nad odciskiem CAŁEGO receiptu. */
  signature?: string;
  /** M1: opcjonalne wiązanie wejścia/stanu (brak = receipt poziomu 0). */
  wiazania?: WiazaniaM1 | null;
}

/** M1: zadanie klienta (tokeny + tokenizer + nonce) do bramki wejścia. */
export interface ZadanieM1 {
  tokenizer_hash: string;
  client_nonce: string;
  prompt_token_ids: number[];
  output_token_ids: number[];
}

/** Werdykt bramki M1. */
export interface WerdyktM1 {
  ok: boolean;
  poziom: number;
  powod: string | null;
}

/** Audyt M3: krok = token node'a vs top-k verifiera. */
export interface KrokM3 {
  indeks: number;
  node_token: number;
  topk: [number, number][];
}

/** Werdykt containment M3. */
export interface WerdyktM3 {
  krokow: number;
  w_topk: number;
  poza_topk: number;
  najgorszy_rank: number;
  najgorszy_margin: number;
  ok: boolean;
  powod: string | null;
}

/** Werdykt zwracany przez WASM (`simon-verify-wasm`) — osobne bramki. */
export interface Verdict {
  parsuje_sie: boolean;
  podpis_ok: boolean;
  tresc_ok: boolean | null;
  job_id_ok: boolean | null;
  model_ok: boolean | null;
  ok: boolean;
  job_id: string | null;
  node_id: string | null;
  model_hash: string | null;
  runtime: string | null;
  signer: string | null;
  output_digest: string | null;
  powod: string | null;
}
