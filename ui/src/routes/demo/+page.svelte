<script lang="ts">
  import { RECEIPT_JSON, OUTPUT } from '$lib/fixture';
  import type { Receipt } from '$lib/receipt';
  import NodePanel from '$lib/components/NodePanel.svelte';
  import VerifierPanel from '$lib/components/VerifierPanel.svelte';

  const receipt: Receipt = JSON.parse(RECEIPT_JSON) as Receipt;
  let output = $state(OUTPUT);
  let tampered = $state(false);
  let scan = $state(true);

  // Appends one space to the output; the signed receipt stays untouched.
  function tamper() { output = output + ' '; tampered = true; }
  function restore() { output = OUTPUT; tampered = false; }
</script>

<svelte:head><title>SIMON demo: tamper, then verify</title></svelte:head>

<div class="mx-auto max-w-6xl px-4 py-8">
  <div class="mb-6 flex flex-wrap items-end justify-between gap-3">
    <p class="max-w-xl text-sm text-neutral-400">
      The node signed the digest of the <em>whole</em> receipt — job id, model, precision and
      the output commitment. Change the answer, then verify: the same Rust verifier as the
      node harness re-derives the output digest and checks the Ed25519 signature.
    </p>
    <label class="mono flex cursor-pointer items-center gap-2 text-xs text-neutral-500">
      <input type="checkbox" bind:checked={scan} class="accent-green-500" /> scanlines
    </label>
  </div>

  <div class="grid gap-4 md:grid-cols-2" class:no-scan={!scan}>
    <NodePanel {receipt} {output} {tampered} ontamper={tamper} onrestore={restore} />
    <VerifierPanel receiptJson={RECEIPT_JSON} {receipt} {output} />
  </div>
</div>

<style>
  .no-scan :global(.scanlines)::after { display: none; }
</style>
