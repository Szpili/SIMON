<script lang="ts">
  import { VALID_RECEIPT, PROMPT } from '$lib/fixture';
  import type { Receipt } from '$lib/receipt';
  import NodePanel from '$lib/components/NodePanel.svelte';
  import VerifierPanel from '$lib/components/VerifierPanel.svelte';

  let receipt = $state<Receipt>({ ...VALID_RECEIPT });
  let tampered = $state(false);
  let scan = $state(true);

  // Appends one space to the output; hash and signature stay as the node signed them.
  function tamper() { receipt = { ...receipt, output: receipt.output + ' ' }; tampered = true; }
  function restore() { receipt = { ...VALID_RECEIPT }; tampered = false; }
</script>

<svelte:head><title>SIMON demo: tamper, then verify</title></svelte:head>

<div class="mx-auto max-w-6xl px-4 py-8">
  <div class="mb-6 flex flex-wrap items-end justify-between gap-3">
    <p class="max-w-xl text-sm text-neutral-400">
      The node signed the hash of its output. Change the output, then verify. The check recomputes the hash and refuses any mismatch.
    </p>
    <label class="mono flex cursor-pointer items-center gap-2 text-xs text-neutral-500">
      <input type="checkbox" bind:checked={scan} class="accent-green-500" /> scanlines
    </label>
  </div>

  <div class="grid gap-4 md:grid-cols-2" class:no-scan={!scan}>
    <NodePanel {receipt} prompt={PROMPT} {tampered} ontamper={tamper} onrestore={restore} />
    <VerifierPanel {receipt} />
  </div>
</div>

<style>
  .no-scan :global(.scanlines)::after { display: none; }
</style>
