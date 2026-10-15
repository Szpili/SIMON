<script lang="ts">
  import {
    RECEIPT_JSON, OUTPUT, RECEIPT_M1_JSON, ZADANIE_M1, OUTPUT_M1, AUDYT_M3_JSON
  } from '$lib/fixture';
  import type { Receipt } from '$lib/receipt';
  import NodePanel from '$lib/components/NodePanel.svelte';
  import VerifierPanel from '$lib/components/VerifierPanel.svelte';

  let poziom = $state<0 | 1>(1);
  let scan = $state(true);

  function flipHex(sig: string): string {
    if (!sig || sig.length < 2) return sig;
    const flipChar = (c: string) => (c.toLowerCase() === '0' ? 'f' : '0');
    return flipChar(sig[0]) + flipChar(sig[1]) + sig.slice(2);
  }

  // level 0 — legacy: the node signed the whole output text.
  let r0 = $state<Receipt>(JSON.parse(RECEIPT_JSON) as Receipt);
  let output0 = $state(OUTPUT);
  let tampered0 = $state(false);
  const r0Json = $derived(JSON.stringify(r0));
  function tamper0() { output0 = output0 + ' '; tampered0 = true; }
  function tamperSigL0() {
    if (r0.signature) {
      r0.signature = flipHex(r0.signature);
      tampered0 = true;
    }
  }
  function restore0() {
    r0 = JSON.parse(RECEIPT_JSON) as Receipt;
    output0 = OUTPUT;
    tampered0 = false;
  }

  // level 1 — M1: the node committed to the exact output TOKENS.
  let r1 = $state<Receipt>(JSON.parse(RECEIPT_M1_JSON) as Receipt);
  let outTok = $state<number[]>([...ZADANIE_M1.output_token_ids]);
  let tampered1 = $state(false);
  const r1Json = $derived(JSON.stringify(r1));
  function tamper1() { outTok = outTok.map((t, i) => (i === 5 ? t + 1 : t)); tampered1 = true; }
  function tamperSigL1() {
    if (r1.signature) {
      r1.signature = flipHex(r1.signature);
      tampered1 = true;
    }
  }
  function restore1() {
    r1 = JSON.parse(RECEIPT_M1_JSON) as Receipt;
    outTok = [...ZADANIE_M1.output_token_ids];
    tampered1 = false;
  }

  const m1Wejscie = $derived({
    tokenizerHash: ZADANIE_M1.tokenizer_hash,
    clientNonce: ZADANIE_M1.client_nonce,
    promptTokens: ZADANIE_M1.prompt_token_ids,
    outputTokens: outTok
  });
  const tokWidok = $derived(
    'tokens: [' + outTok.slice(0, 12).join(', ') + (outTok.length > 12 ? ', …]' : ']')
  );
</script>

<svelte:head><title>SIMON demo: tamper, then verify</title></svelte:head>

<div class="mx-auto max-w-6xl px-4 py-8">
  <div class="mb-6 flex flex-wrap items-end justify-between gap-3">
    <p class="max-w-2xl text-sm text-neutral-400">
      The node signs the digest of the whole receipt. Change the answer (level 0), one output
      <em>token</em> (level 1), or tamper with the Ed25519 signature, then verify — the same Rust verifier as the CLI checks the signature,
      the output binding, the M1 input/state binding and the M3 top-k execution audit.
    </p>
    <div class="flex items-center gap-3">
      <div class="mono flex text-xs">
        <button
          class="border px-3 py-1 {poziom === 0 ? 'border-valid text-valid' : 'border-line text-neutral-500'}"
          onclick={() => (poziom = 0)}>level 0</button>
        <button
          class="border border-l-0 px-3 py-1 {poziom === 1 ? 'border-valid text-valid' : 'border-line text-neutral-500'}"
          onclick={() => (poziom = 1)}>level 1 (M1+M3)</button>
      </div>
      <label class="mono flex cursor-pointer items-center gap-2 text-xs text-neutral-500">
        <input type="checkbox" bind:checked={scan} class="accent-green-500" /> scanlines
      </label>
    </div>
  </div>

  <div class="grid gap-4 md:grid-cols-2" class:no-scan={!scan}>
    {#if poziom === 0}
      <NodePanel receipt={r0} output={output0} tampered={tampered0} ontamper={tamper0} onTamperSig={tamperSigL0} onrestore={restore0} />
      <VerifierPanel receiptJson={r0Json} receipt={r0} output={output0} />
    {:else}
      <NodePanel receipt={r1} output={tokWidok} tampered={tampered1} ontamper={tamper1} onTamperSig={tamperSigL1} onrestore={restore1} tamperNote="Output token 6 was changed by one." />
      <VerifierPanel receiptJson={r1Json} receipt={r1} output={OUTPUT_M1} m1={m1Wejscie} auditJson={AUDYT_M3_JSON} />
    {/if}
  </div>

  {#if poziom === 1}
    <p class="mt-3 mono text-xs text-neutral-500">
      level 1: receipt commits the exact prompt token ids (M1); tamper flips one output token; the M3
      audit then checks the node's tokens against the verifier's top-k.
    </p>
  {/if}
</div>

<style>
  .no-scan :global(.scanlines)::after { display: none; }
</style>
