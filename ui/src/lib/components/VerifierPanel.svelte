<script lang="ts">
  import { onMount } from 'svelte';
  import type { Receipt, Verdict, WerdyktM1, WerdyktM3 } from './../receipt';
  import { verifyReceipt, outputDigest, verifyM1, ocenaM3 } from './../verify';
  import StatusPill from './StatusPill.svelte';
  import CheckStep from './CheckStep.svelte';

  type M1Wejscie = {
    tokenizerHash: string;
    clientNonce: string;
    promptTokens: number[];
    outputTokens: number[];
  };

  let {
    receiptJson,
    receipt,
    output,
    m1 = null,
    auditJson = null
  }: {
    receiptJson: string;
    receipt: Receipt;
    output: string | null;
    m1?: M1Wejscie | null;
    auditJson?: string | null;
  } = $props();

  type Step = { label: string; status: 'pass' | 'fail'; detail: string };
  let steps = $state<Step[]>([]);
  let shown = $state(0);
  let pill = $state<'idle' | 'running' | 'valid' | 'rejected'>('idle');
  let showJson = $state(false);
  let busy = $state(false);
  const poziom = $derived(receipt.wiazania?.receipt_level ?? 0);
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const short = (s: string) => (s.length > 26 ? s.slice(0, 12) + '…' + s.slice(-8) : s);

  async function run() {
    if (busy) return;
    busy = true; shown = 0; pill = 'running';

    let v: Verdict;
    let computed: string | null = null;
    let m1v: WerdyktM1 | null = null;
    let m3v: WerdyktM3 | null = null;
    try {
      v = await verifyReceipt(receiptJson, output);
      try { computed = output ? await outputDigest(receipt.job_id, output) : null; } catch { computed = null; }
      if (m1) m1v = await verifyM1(receiptJson, m1.tokenizerHash, m1.clientNonce, m1.promptTokens, m1.outputTokens);
      if (auditJson) m3v = await ocenaM3(auditJson);
    } catch (e) {
      v = {
        parsuje_sie: false, podpis_ok: false, tresc_ok: null, job_id_ok: null, model_ok: null,
        ok: false, job_id: null, node_id: null, model_hash: null, runtime: null, signer: null,
        output_digest: null, powod: String(e)
      };
    }
    steps = [
      {
        label: 'Parse the receipt (schema + node identity)',
        status: v.parsuje_sie ? 'pass' : 'fail',
        detail: v.parsuje_sie ? `job ${short(v.job_id ?? '')}, model ${v.model_hash}` : (v.powod ?? 'not a receipt')
      },
      {
        label: 'Ed25519 signature over the whole receipt',
        status: v.podpis_ok ? 'pass' : 'fail',
        detail: v.podpis_ok ? `signer ${short(v.signer ?? '')}` : 'signature does not match the receipt digest'
      },
      {
        label: 'Output matches the signed digest (job_id-bound)',
        status: v.tresc_ok ? 'pass' : 'fail',
        detail: v.tresc_ok
          ? (computed ? short(computed) : 'identical')
          : `signed ${short(v.output_digest ?? '')} ≠ computed ${short(computed ?? '')}`
      }
    ];

    if (m1v) {
      steps = [...steps, {
        label: 'M1: exact input + state binding (tokenizer, nonce, tokens)',
        status: m1v.ok ? 'pass' : 'fail',
        detail: m1v.ok ? 'tokenizer + nonce + prompt tokens + output tokens committed' : (m1v.powod ?? 'binding mismatch')
      }];
    }
    if (m3v) {
      steps = [...steps, {
        label: 'M3: node tokens within verifier top-k (execution audit)',
        status: m3v.ok ? 'pass' : 'fail',
        detail: m3v.ok
          ? `${m3v.w_topk}/${m3v.krokow} in top-k, worst rank ${m3v.najgorszy_rank}, margin ${m3v.najgorszy_margin.toFixed(3)}`
          : (m3v.powod ?? `${m3v.poza_topk}/${m3v.krokow} suspicious`)
      }];
    }

    for (let i = 1; i <= steps.length; i++) { await sleep(160); shown = i; }
    pill = v.ok && (m1v?.ok ?? true) && (m3v?.ok ?? true) ? 'valid' : 'rejected';
    busy = false;
  }

  onMount(run);
</script>

<section class="scanlines flex flex-col border border-line bg-panel p-5" aria-labelledby="ver-h">
  <div class="flex items-baseline justify-between">
    <h2 id="ver-h" class="mono text-base font-bold text-neutral-100">Verifier</h2>
    <span class="mono text-xs text-neutral-500">receipt level {poziom}</span>
  </div>

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

  <p class="mt-4 text-sm text-neutral-400">
    This ran entirely in your browser. No network calls. No server. Signature, output digest,
    M1 input/state binding and the M3 top-k containment check are all done by the
    <span class="mono">simon-core</span> Rust verifier compiled to WebAssembly.
  </p>

  {#if showJson}
    <pre class="mono mt-4 max-h-72 overflow-auto whitespace-pre-wrap break-all border border-line bg-ink p-3 text-xs text-neutral-300">{JSON.stringify(JSON.parse(receiptJson), null, 2)}</pre>
  {/if}
</section>
