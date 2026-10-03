<script lang="ts">
  import type { Receipt } from './../receipt';
  import HashField from './HashField.svelte';
  let { receipt, output, tampered, ontamper, onrestore }: {
    receipt: Receipt; output: string; tampered: boolean; ontamper: () => void; onrestore: () => void;
  } = $props();
</script>

<section class="scanlines flex flex-col border border-line bg-panel p-5" aria-labelledby="node-h">
  <div class="flex items-center justify-between">
    <h2 id="node-h" class="mono text-base font-bold text-neutral-100">Untrusted Node</h2>
    <span class="mono text-xs {tampered ? 'text-bad' : 'text-neutral-500'}">{tampered ? 'output modified' : 'as served'}</span>
  </div>

  <div class="mt-5">
    <p class="mono text-xs text-neutral-500">output (the answer it returned)</p>
    <p class="mono mt-1 whitespace-pre-wrap break-words border p-3 text-sm {tampered ? 'border-bad/60' : 'border-line'} bg-ink text-neutral-100">{output}<span class="{tampered ? 'bg-bad' : ''}">{tampered ? '\u00a0' : ''}</span></p>
    {#if tampered}<p class="mono mt-1 text-xs text-bad">One trailing space was appended. The signed receipt was not re-issued.</p>{/if}
  </div>

  <div class="mt-4">
    <HashField label="signature (Ed25519)" value={receipt.signature ?? ''} head={16} tail={6} />
    <HashField label="signer pubkey" value={receipt.signer} head={16} tail={6} />
    <HashField label="signed output digest" value={receipt.output_digest} head={16} tail={6} />
  </div>

  <div class="mt-4 grid grid-cols-2 gap-x-4 gap-y-1 mono text-xs text-neutral-500">
    <span class="truncate">job: <span class="text-neutral-300">{receipt.job_id}</span></span>
    <span class="truncate">model: <span class="text-neutral-300">{receipt.model_hash}</span></span>
    <span class="truncate">runtime: <span class="text-neutral-300">{receipt.runtime}</span></span>
    <span class="truncate">precision: <span class="text-neutral-300">{receipt.precision}</span></span>
  </div>

  <div class="mt-5 flex flex-wrap gap-3">
    <button type="button" class="btn btn-danger" onclick={ontamper} disabled={tampered}>Tamper with output</button>
    <button type="button" class="btn" onclick={onrestore} disabled={!tampered}>Restore original</button>
  </div>
</section>
