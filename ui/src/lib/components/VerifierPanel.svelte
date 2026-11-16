<script lang="ts">
  import { onMount } from 'svelte';
  import type { Receipt, Verdict, M1Verdict, M3Verdict } from './../receipt';
  import { verifyReceipt, outputDigest, verifyM1, evaluateM3 } from './../verify';
  import StatusPill from './StatusPill.svelte';
  import CheckStep from './CheckStep.svelte';

  type M1Input = {
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
    m1?: M1Input | null;
    auditJson?: string | null;
  } = $props();

  type Step = { label: string; status: 'pass' | 'fail'; detail: string };
  let steps = $state<Step[]>([]);
  let shown = $state(0);
  let pill = $state<'idle' | 'running' | 'valid' | 'rejected'>('idle');
  let showJson = $state(false);
  let busy = $state(false);
  const level = $derived(receipt.bindings?.receipt_level ?? 0);
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const short = (s: string) => (s.length > 26 ? s.slice(0, 12) + '…' + s.slice(-8) : s);

  async function run() {
    if (busy) return;
    busy = true; shown = 0; pill = 'running';

    let v: Verdict;
    let computed: string | null = null;
    let m1v: M1Verdict | null = null;
    let m3v: M3Verdict | null = null;
    try {
      v = await verifyReceipt(receiptJson, output);
      try { computed = output ? await outputDigest(receipt.job_id, output) : null; } catch { computed = null; }
      if (m1) m1v = await verifyM1(receiptJson, m1.tokenizerHash, m1.clientNonce, m1.promptTokens, m1.outputTokens);
      if (auditJson) m3v = await evaluateM3(auditJson);
    } catch (e) {
      v = {
        parses: false, signature_ok: false, content_ok: null, job_id_ok: null, model_ok: null,
        ok: false, job_id: null, node_id: null, model_hash: null, runtime: null, signer: null,
        output_digest: null, reason: String(e)
      };
    }
    const parsesOk = v.parses ?? v.parsuje_sie ?? false;
    const sigOk = v.signature_ok ?? v.podpis_ok ?? false;
    const contentOk = v.content_ok ?? v.tresc_ok ?? null;
    const reasonText = v.reason ?? v.powod ?? null;

    steps = [
      {
        label: 'Parse the receipt (schema + node identity)',
        status: parsesOk ? 'pass' : 'fail',
        detail: parsesOk ? `job ${short(v.job_id ?? '')}, model ${v.model_hash}` : (reasonText ?? 'not a receipt')
      },
      {
        label: 'Ed25519 signature over the whole receipt',
        status: sigOk ? 'pass' : 'fail',
        detail: sigOk ? `signer ${short(v.signer ?? '')}` : 'signature does not match the receipt digest'
      },
      {
        label: 'Output matches the signed digest (job_id-bound)',
        status: contentOk ? 'pass' : 'fail',
        detail: contentOk
          ? (computed ? short(computed) : 'identical')
          : `signed ${short(v.output_digest ?? '')} ≠ computed ${short(computed ?? '')}`
      }
    ];

    if (m1v) {
      const m1Reason = m1v.reason ?? m1v.powod ?? null;
      steps = [...steps, {
        label: 'M1: exact input + state binding (tokenizer, nonce, tokens)',
        status: m1v.ok ? 'pass' : 'fail',
        detail: m1v.ok ? 'tokenizer + nonce + prompt tokens + output tokens committed' : (m1Reason ?? 'binding mismatch')
      }];
    }
    if (m3v) {
      const inTopk = m3v.in_topk ?? m3v.w_topk ?? 0;
      const stepsCount = m3v.steps ?? m3v.krokow ?? 0;
      const worstRank = m3v.worst_rank ?? m3v.najgorszy_rank ?? 0;
      const worstMargin = m3v.worst_margin ?? m3v.najgorszy_margin ?? 0;
      const outsideTopk = m3v.outside_topk ?? m3v.poza_topk ?? 0;
      const m3Reason = m3v.reason ?? m3v.powod ?? null;
      steps = [...steps, {
        label: 'M3: node tokens within verifier top-k (telemetry, does not gate the verdict)',
        status: m3v.ok ? 'pass' : 'fail',
        detail: m3v.ok
          ? `${inTopk}/${stepsCount} in top-k, worst rank ${worstRank}, margin ${worstMargin.toFixed(3)}`
          : (m3Reason ?? `${outsideTopk}/${stepsCount} suspicious`)
      }];
    }

    for (let i = 1; i <= steps.length; i++) { await sleep(160); shown = i; }
    // M3 failed its zero-false-positive gate (docs/FP-RESULT.md): shown, never decides the verdict.
    pill = v.ok && (m1v?.ok ?? true) ? 'valid' : 'rejected';
    busy = false;
  }

  onMount(run);
</script>

<section class="scanlines flex flex-col border border-line bg-panel p-5" aria-labelledby="ver-h">
  <div class="flex items-baseline justify-between">
    <h2 id="ver-h" class="mono text-base font-bold text-neutral-100">Verifier</h2>
    <span class="mono text-xs text-neutral-500">receipt level {level}</span>
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
