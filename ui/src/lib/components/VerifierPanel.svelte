<script lang="ts">
  import { onMount } from 'svelte';
  import type { Receipt } from './../receipt';
  import { verifyDetailed } from './../crypto';
  import StatusPill from './StatusPill.svelte';
  import CheckStep from './CheckStep.svelte';

  let { receipt }: { receipt: Receipt } = $props();

  type Step = { label: string; status: 'pass' | 'fail'; detail: string };
  let steps = $state<Step[]>([]);
  let shown = $state(0);
  let pill = $state<'idle' | 'running' | 'valid' | 'rejected'>('idle');
  let showJson = $state(false);
  let busy = $state(false);
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const short = (s: string) => (s.length > 24 ? s.slice(0, 12) + '…' + s.slice(-8) : s);

  async function run() {
    if (busy) return;
    busy = true; shown = 0; pill = 'running';
    const d = verifyDetailed($state.snapshot(receipt) as Receipt);
    steps = [
      { label: 'Fetch receipt from node', status: 'pass', detail: `job ${short(receipt.job_id)}, model ${receipt.model}` },
      { label: 'Verify Ed25519 signature against node pubkey', status: d.signatureOk ? 'pass' : 'fail', detail: d.signatureOk ? 'signature matches the signed hash' : 'signature does not match' },
      { label: 'Recompute SHA-256 of output', status: 'pass', detail: short(d.recomputedHash) },
      { label: 'Compare signed hash vs. recomputed hash', status: d.hashesMatch ? 'pass' : 'fail', detail: d.hashesMatch ? 'identical' : `signed ${short(receipt.output_sha256)} is not ${short(d.recomputedHash)}` }
    ];
    for (let i = 1; i <= steps.length; i++) { await sleep(200); shown = i; }
    pill = d.valid ? 'valid' : 'rejected';
    busy = false;
  }

  onMount(run);
</script>

<section class="scanlines flex flex-col border border-line bg-panel p-5" aria-labelledby="ver-h">
  <h2 id="ver-h" class="mono text-base font-bold text-neutral-100">Verifier</h2>

  <div class="mt-5"><StatusPill state={pill} /></div>

  <ol class="mt-5 space-y-2">
    {#each steps as s, i}
      <CheckStep label={s.label} status={s.status} detail={s.detail} shown={i < shown} />
    {/each}
  </ol>

  <div class="mt-5 flex flex-wrap items-center gap-3">
    <button type="button" class="btn btn-primary" onclick={run} disabled={busy}>Verify offline</button>
    <button type="button" class="btn" onclick={() => (showJson = !showJson)} aria-expanded={showJson}>
      {showJson ? 'Hide source receipt' : 'View source receipt'}
    </button>
  </div>

  <p class="mt-4 text-sm text-neutral-400">This ran entirely in your browser. No network calls. No server. Just math.</p>

  {#if showJson}
    <pre class="mono mt-4 max-h-72 overflow-auto whitespace-pre-wrap break-all border border-line bg-ink p-3 text-xs text-neutral-300">{JSON.stringify(receipt, null, 2)}</pre>
  {/if}
</section>
