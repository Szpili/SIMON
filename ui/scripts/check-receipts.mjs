// Verifies every stored /industry receipt with the COMMITTED wasm (ui/src/lib/wasm), as the browser does.
// Why (2026-10-17): the node started writing precision "int4"; the committed wasm predated it and rejected
// 8 of 15 receipts on the live page while every Rust test was green. Fails CI if the wasm is stale.
import { readFileSync } from 'node:fs';
import init, { weryfikuj } from '../src/lib/wasm/simon_verify.js';

const here = new URL('.', import.meta.url);
await init({ module_or_path: readFileSync(new URL('../src/lib/wasm/simon_verify_bg.wasm', here)) });
const run = JSON.parse(readFileSync(new URL('../src/lib/industry-run.json', here), 'utf8'));

let bad = 0, n = 0;
for (const r of run.records) {
  if (!r.receipt || !r.signed_output) continue;
  n++;
  const v = JSON.parse(weryfikuj(JSON.stringify(r.receipt), r.signed_output, undefined, undefined));
  if (!v.ok) { bad++; console.error(`record ${r.record}: ${v.powod}`); }
}
console.log(`industry receipts verified by the committed wasm: ${n - bad}/${n}`);
if (bad || !n) process.exit(1);
