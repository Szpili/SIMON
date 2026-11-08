/* tslint:disable */
/* eslint-disable */

/**
 * Gate M3: evaluates execution audit (containment in top-k + margin).
 */
export function evaluate_m3(audit_json: string, k: number, max_margin: number, max_outside: number): string;

/**
 * Canonical output digest matching receipt expectation.
 */
export function output_digest(job_id: string, output: string): string | undefined;

/**
 * Verifies receipt (JSON) against optional output content and optional expected job_id / model_hash.
 */
export function verify_receipt(receipt_json: string, output?: string | null, expected_job_id?: string | null, expected_model?: string | null): string;

/**
 * Gate M1: checks if receipt binds exactly the tokenizer, nonce, prompt, and output tokens.
 */
export function verify_m1(receipt_json: string, tokenizer_hash: string, client_nonce: string, prompt_token_ids_json: string, output_token_ids_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly evaluate_m3: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly output_digest: (a: number, b: number, c: number, d: number) => [number, number];
    readonly verify_receipt: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number];
    readonly verify_m1: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
