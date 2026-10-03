<script lang="ts">
  import {
    RECEIPT_JSON, OUTPUT, RECEIPT_M1_JSON, ZADANIE_M1, OUTPUT_M1, AUDYT_M3_JSON
  } from '$lib/fixture';
  import type { Receipt } from '$lib/receipt';
  import NodePanel from '$lib/components/NodePanel.svelte';
  import VerifierPanel from '$lib/components/VerifierPanel.svelte';

  let poziom = $state<0 | 1>(1);
  let scan = $state(true);

  // level 0 — legacy: the node signed the whole output text.
  const receipt0: Receipt = JSON.parse(RECEIPT_JSON) as Receipt;
  let output0 = $state(OUTPUT);
  let tampered0 = $state(false);
  function tamper0() { output0 = output0 + ' '; tampered0 = true; }
  function restore0() { output0 = OUTPUT; tampered0 = false; }

  // level 1 — M1: the node committed to the exact output TOKENS.
  const receipt1: Receipt = JSON.parse(RECEIPT_M1_JSON) as Receipt;
  let outTok = $state<number[]>([...ZADANIE_M1.output_token_ids]);
  let tampered1 = $state(false);
  function tamper1() { outTok = outTok.map((t, i) => (i === 5 ? t + 1 : t)); tampered1 = true; }
  function restore1() { outTok = [...ZADANIE_M1.output_token_ids]; tampered1 = false; }

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
      The node signs the digest of the whole receipt. Change the answer (level 0) or one output
      <em>token</em> (level 1), then verify — the same Rust verifier as the CLI checks the signature,
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
      <NodePanel receipt={receipt0} output={output0} tampered={tampered0} ontamper={tamper0} onrestore={restore0} />
      <VerifierPanel receiptJson={RECEIPT_JSON} receipt={receipt0} output={output0} />
    {:else}
      <NodePanel receipt={receipt1} output={tokWidok} tampered={tampered1} ontamper={tamper1} onrestore={restore1} />
      <VerifierPanel receiptJson={RECEIPT_M1_JSON} receipt={receipt1} output={OUTPUT_M1} m1={m1Wejscie} auditJson={AUDYT_M3_JSON} />
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
