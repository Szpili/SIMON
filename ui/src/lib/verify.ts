// Weryfikacja receiptu = wywołanie WASM z `simon-core` (jedno źródło prawdy).
//
// Zero własnej kryptografii po stronie JS. `sha256(output)` i `ed.verify(...)`
// z usuniętego `crypto.ts` były DRUGĄ implementacją i rozjeżdżały się z rdzeniem
// (rdzeń podpisuje odcisk CAŁEGO receiptu, nie hash wyjścia).
//
// WASM ładujemy leniwie, wyłącznie w przeglądarce — strona jest prerenderowana
// (adapter-static), więc import modułu i `init()` nie mogą dziać się na serwerze.

import type { Verdict, WerdyktM1, WerdyktM3 } from './receipt';

type WasmModule = typeof import('./wasm/simon_verify.js');

let got: Promise<WasmModule> | null = null;

function load(): Promise<WasmModule> {
  if (!got) {
    got = import('./wasm/simon_verify.js').then(async (m) => {
      await m.default(); // inicjalizacja wasm-bindgen (fetch lokalnego .wasm)
      return m;
    });
  }
  return got;
}

/**
 * Weryfikuje receipt (JSON) wobec treści odpowiedzi i opcjonalnych oczekiwań.
 * Dokładnie te same bramki co `simon --verify-receipt` w CLI.
 */
export async function verifyReceipt(
  receiptJson: string,
  output: string | null,
  expectJobId?: string,
  expectModel?: string
): Promise<Verdict> {
  const m = await load();
  const raw = m.weryfikuj(
    receiptJson,
    output ?? undefined,
    expectJobId ?? undefined,
    expectModel ?? undefined
  );
  return JSON.parse(raw) as Verdict;
}

/** Kanoniczny odcisk treści (SIMON/OUTPUT/v1) — do podglądu, liczy Rust. */
export async function outputDigest(jobId: string, output: string): Promise<string | null> {
  const m = await load();
  return m.odcisk_wyjscia(jobId, output) ?? null;
}

/** Bramka M1: tokenizer + nonce + prompt + wyjście po tokenach (CPU, bez GPU). */
export async function verifyM1(
  receiptJson: string,
  tokenizerHash: string,
  clientNonce: string,
  promptTokens: number[],
  outputTokens: number[]
): Promise<WerdyktM1> {
  const m = await load();
  const raw = m.weryfikuj_m1(
    receiptJson,
    tokenizerHash,
    clientNonce,
    JSON.stringify(promptTokens),
    JSON.stringify(outputTokens)
  );
  return JSON.parse(raw) as WerdyktM1;
}

/** Bramka M3: ocena audytu containment (top-k + margines). */
export async function ocenaM3(
  audytJson: string,
  k = 2,
  maxMargin = 0.25,
  maxPoza = 0.02
): Promise<WerdyktM3> {
  const m = await load();
  const raw = m.ocena_m3(audytJson, k, maxMargin, maxPoza);
  return JSON.parse(raw) as WerdyktM3;
}
