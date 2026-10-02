<script lang="ts">
  import type { Receipt } from './../receipt';
  import HashField from './HashField.svelte';
  let { receipt, prompt, tampered, ontamper, onrestore }: {
    receipt: Receipt; prompt: string; tampered: boolean; ontamper: () => void; onrestore: () => void;
  } = $props();
</script>

<section class="scanlines flex flex-col border border-line bg-panel p-5" aria-labelledby="node-h">
  <div class="flex items-center justify-between">
    <h2 id="node-h" class="mono text-base font-bold text-neutral-100">Untrusted Node</h2>
    <span class="mono text-xs {tampered ? 'text-bad' : 'text-neutral-500'}">{tampered ? 'output modified' : 'as served'}</span>
  </div>

  <div class="mt-5">
    <p class="mono text-xs text-neutral-500">prompt</p>
    <p class="mono mt-1 border border-line bg-ink p-3 text-sm text-neutral-300">{prompt}</p>
  </div>

  <div class="mt-4">
    <p class="mono text-xs text-neutral-500">output</p>
    <p class="mono mt-1 whitespace-pre-wrap break-words border p-3 text-sm {tampered ? 'border-bad/60' : 'border-line'} bg-ink text-neutral-100">{receipt.output}<span class="{tampered ? 'bg-bad' : ''}">{tampered ? '\u00a0' : ''}</span></p>
    {#if tampered}<p class="mono mt-1 text-xs text-bad">One trailing space was appended. The signature was not re-issued.</p>{/if}
  </div>

  <div class="mt-4">
    <HashField label="signature" value={receipt.signature} head={16} tail={6} />
    <HashField label="node pubkey" value={receipt.node_pubkey} head={16} tail={6} />
    <HashField label="signed hash" value={receipt.output_sha256} head={12} tail={6} />
  </div>

  <div class="mt-5 flex flex-wrap gap-3">
    <button type="button" class="btn btn-danger" onclick={ontamper} disabled={tampered}>Tamper with output</button>
    <button type="button" class="btn" onclick={onrestore} disabled={!tampered}>Restore original</button>
  </div>
</section>
