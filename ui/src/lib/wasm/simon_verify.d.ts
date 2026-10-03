/* tslint:disable */
/* eslint-disable */

/**
 * Bramka M3: ocenia audyt wykonania (containment w top-k + margines).
 * `audyt_json` = `{"kroki":[{indeks,node_token,topk:[[id,lp],..]}]}`.
 */
export function ocena_m3(audyt_json: string, k: number, max_margin: number, max_poza: number): string;

/**
 * Odcisk treści wyniku, liczący się z receiptem — do podglądu/diagnostyki.
 * Ten sam wzór co w `odcisk_wyjscia` rdzenia.
 */
export function odcisk_wyjscia(job_id: string, output: string): string | undefined;

/**
 * Weryfikuje receipt (JSON) wobec opcjonalnej treści odpowiedzi oraz
 * opcjonalnego oczekiwanego `job_id` / `model_hash`. Zwraca JSON z osobnymi
 * bramkami — UI wyłącznie prezentuje wynik.
 *
 * * `receipt_json` — receipt w formacie SIMON (JSON),
 * * `output`       — treść odpowiedzi, którą rzekomo opisuje receipt
 *                    (bez niej bramka treści jest `null` = nie sprawdzona),
 * * `oczekiwany_job_id` / `oczekiwany_model` — opcjonalne bramki zgodności.
 */
export function weryfikuj(receipt_json: string, output?: string | null, oczekiwany_job_id?: string | null, oczekiwany_model?: string | null): string;

/**
 * Bramka M1: czy receipt wiąże DOKŁADNIE ten tokenizer, nonce, prompt i wyjście
 * (po tokenach). Tokeny podajemy jako JSON-owe tablice u32. Czysty CPU — bez
 * wag i bez GPU. To ta sama logika co `Receipt::zweryfikuj_m1` w rdzeniu.
 */
export function weryfikuj_m1(receipt_json: string, tokenizer_hash: string, client_nonce: string, prompt_token_ids_json: string, output_token_ids_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly ocena_m3: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly odcisk_wyjscia: (a: number, b: number, c: number, d: number) => [number, number];
    readonly weryfikuj: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number];
    readonly weryfikuj_m1: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
