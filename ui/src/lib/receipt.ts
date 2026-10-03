//! Kształt receiptu — zgodny 1:1 z `simon-core::receipt::Receipt`.
//!
//! Świadomie NIE ma tu logiki kryptograficznej. Weryfikacja żyje w Rust
//! (`simon-core`, eksport WASM w `./wasm/`); ten plik tylko opisuje dane.

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
