// Receipt verification via WASM compiled from `simon-core` (single source of truth).
//
// Zero bespoke cryptography in JavaScript. The WASM module is loaded lazily
// in the browser (adapter-static prerender cannot run WASM at build time).

import type { Verdict, M1Verdict, M3Verdict, WerdyktM1, WerdyktM3 } from './receipt';

type WasmModule = typeof import('./wasm/simon_verify.js');

let got: Promise<WasmModule> | null = null;

function load(): Promise<WasmModule> {
  if (!got) {
    got = import('./wasm/simon_verify.js').then(async (m) => {
      await m.default();
      return m;
    });
  }
  return got;
}

/**
 * Verifies receipt (JSON) against output text and optional expected values.
 * Identical gates to `simon --verify-receipt` in CLI.
 */
export async function verifyReceipt(
  receiptJson: string,
  output: string | null,
  expectJobId?: string,
  expectModel?: string
): Promise<Verdict> {
  const m = await load();
  const fn = (m as any).verify_receipt ?? (m as any).weryfikuj;
  const raw = fn(
    receiptJson,
    output ?? undefined,
    expectJobId ?? undefined,
    expectModel ?? undefined
  );
  return JSON.parse(raw) as Verdict;
}

/** Canonical output digest (SIMON/OUTPUT/v1) computed by Rust. */
export async function outputDigest(jobId: string, output: string): Promise<string | null> {
  const m = await load();
  const fn = (m as any).output_digest ?? (m as any).odcisk_wyjscia;
  return fn(jobId, output) ?? null;
}

/** M1 gate: tokenizer + nonce + prompt + output token IDs on CPU without GPU. */
export async function verifyM1(
  receiptJson: string,
  tokenizerHash: string,
  clientNonce: string,
  promptTokens: number[],
  outputTokens: number[]
): Promise<M1Verdict> {
  const m = await load();
  const fn = (m as any).verify_m1 ?? (m as any).weryfikuj_m1;
  const raw = fn(
    receiptJson,
    tokenizerHash,
    clientNonce,
    JSON.stringify(promptTokens),
    JSON.stringify(outputTokens)
  );
  return JSON.parse(raw) as M1Verdict;
}

/** M3 gate: evaluate containment audit (top-k + margin). */
export async function evaluateM3(
  auditJson: string,
  k = 2,
  maxMargin = 0.25,
  maxOutside = 0.02
): Promise<M3Verdict> {
  const m = await load();
  const fn = (m as any).evaluate_m3 ?? (m as any).ocena_m3;
  const raw = fn(auditJson, k, maxMargin, maxOutside);
  return JSON.parse(raw) as M3Verdict;
}
export const ocenaM3 = evaluateM3;
