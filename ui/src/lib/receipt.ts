//! Receipt data types matching `simon-core::receipt::Receipt`.
//!
//! Verification logic lives in Rust (`simon-core`, WASM export in `./wasm/`);
//! this file defines the TypeScript data types.

/** M1: input/state bindings recorded in the receipt (optional). */
export interface M1Bindings {
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
  /** Model hash (D67). */
  model_hash: string;
  /** Runtime version (D67). */
  runtime: string;
  precision: 'bf16' | 'fp16' | 'fp32' | 'fp8' | 'fp4' | 'int8' | 'int4' | 'unknown' | string;
  /** Activation LSH (TopLoc). */
  activation_hash: string;
  /** Domain-separated digest of output content, bound to job_id. */
  output_digest: string;
  prompt_tokens: number;
  completion_tokens: number;
  started_at_us: number;
  finished_at_us: number;
  /** Node public key (hex, 32 B). */
  signer: string;
  /** Ed25519 signature (hex, 64 B) over whole receipt digest. */
  signature?: string;
  /** M1: optional input/state bindings (None = level 0 receipt). */
  bindings?: M1Bindings | null;
}

/** M1: client job inputs (tokens + tokenizer + nonce) for input gate. */
export interface M1Job {
  tokenizer_hash: string;
  client_nonce: string;
  prompt_token_ids: number[];
  output_token_ids: number[];
}

/** M1 gate verdict. */
export interface M1Verdict {
  ok: boolean;
  level: number;
  reason: string | null;
  powod?: string | null;
}

/** M3 audit: step = node token vs verifier top-k. */
export interface M3Step {
  index: number;
  node_token: number;
  topk: [number, number][];
}

/** M3 containment verdict. */
export interface M3Verdict {
  steps: number;
  in_topk: number;
  outside_topk: number;
  worst_rank: number;
  worst_margin: number;
  ok: boolean;
  reason: string | null;
  // Legacy aliases
  krokow?: number;
  w_topk?: number;
  poza_topk?: number;
  najgorszy_rank?: number;
  najgorszy_margin?: number;
  powod?: string | null;
}

/** Verdict returned by WASM (`simon-verify-wasm`). */
export interface Verdict {
  parses: boolean;
  parsuje_sie?: boolean;
  signature_ok: boolean;
  podpis_ok?: boolean;
  content_ok: boolean | null;
  tresc_ok?: boolean | null;
  job_id_ok: boolean | null;
  model_ok: boolean | null;
  ok: boolean;
  job_id: string | null;
  node_id: string | null;
  model_hash: string | null;
  runtime: string | null;
  signer: string | null;
  output_digest: string | null;
  reason: string | null;
  powod?: string | null;
}
