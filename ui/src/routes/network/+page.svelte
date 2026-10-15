<script lang="ts">
  import { onMount } from 'svelte';
  import { base } from '$app/paths';
  import { verifyReceipt } from '$lib/verify';
  import Sparkline from '$lib/components/Sparkline.svelte';

  // Live data comes from tools/simon-monitor (read-only JSON). Same origin by default; ?api=<url> overrides.
  // No fake data: if the monitor is unreachable (e.g. on GitHub Pages) the page says so.
  type Gpu = { name: string; util: number | null; vram_used_mb: number | null; vram_total_mb: number | null;
               temp_c: number | null; power_w: number | null; power_limit_w: number | null };
  type Node = { label: string; model: string | null; shared_with: string | null; telemetry: 'ok' | 'unavailable' | 'remote'; canary: { verdict: string; hits: number | null; of: number | null; age_s: number; served_model: string | null } | null; state: 'active' | 'idle' | 'no_jobs';
                last_job_age_s: number | null; gpu: Gpu | null; history: Record<string, (number | null)[]> };
  type Job = { ts: number; model: string; job_id: string; node_id: string; node_label: string | null;
               completion_tokens: number | null; job_ms: number | null; tok_s: number | null;
               receipt: Record<string, unknown> };
  type Snap = { generated_at: number; nodes: Node[]; jobs: Job[]; totals: { jobs: number; completion_tokens: number } };

  let snap = $state<Snap | null>(null);
  let err = $state<string | null>(null);
  let sig = $state<Record<string, { ok: boolean; why: string | null }>>({});
  const pending = new Set<string>(); // checks in flight, so overlapping polls don't verify twice

  const api = () => new URLSearchParams(location.search).get('api') ?? '/simon-monitor/api/monitor';
  const fmt = (v: number | null | undefined, d = 0) => (v == null ? '–' : v.toFixed(d));
  const short = (s: string | null | undefined) => (s ? `${s.slice(0, 8)}…${s.slice(-6)}` : '–');
  const ago = (ts: number) => { const s = Math.max(0, Math.round(Date.now() / 1000 - ts)); return s < 60 ? `${s}s ago` : `${Math.round(s / 60)}m ago`; };

  async function poll() {
    try {
      const r = await fetch(api(), { cache: 'no-store', signal: AbortSignal.timeout(5000) });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      // a catch-all site in front can answer 200 with HTML: that is not a monitor
      if (!(r.headers.get('content-type') ?? '').includes('json')) throw new Error('no SIMON monitor at this address');
      snap = (await r.json()) as Snap;
      err = null;
      for (const j of snap.jobs) {
        if (sig[j.job_id] || pending.has(j.job_id)) continue;
        pending.add(j.job_id);
        // Signature only: the monitor never ships the answer, so output binding cannot be checked here.
        verifyReceipt(JSON.stringify(j.receipt), null)
          .then((v) => (sig[j.job_id] = { ok: v.parsuje_sie && v.podpis_ok, why: v.powod }))
          .catch((e) => (sig[j.job_id] = { ok: false, why: String(e) }))
          .finally(() => pending.delete(j.job_id));
      }
    } catch (e) {
      err = String(e);
    }
  }

  onMount(() => { poll(); const t = setInterval(poll, 2000); return () => clearInterval(t); });

  const canaryTone = (v: string) => (v === 'ok' ? 'text-valid' : v === 'fail' ? 'text-bad' : 'text-amber-400');
  const canaryText = (c: NonNullable<Node['canary']>) => {
    const when = c.age_s < 60 ? `${c.age_s}s ago` : `${Math.round(c.age_s / 60)} min ago`;
    if (c.verdict === 'ok' || c.verdict === 'warn' || c.verdict === 'fail')
      return `${c.verdict.toUpperCase()} ${c.hits}/${c.of} · ${when} · self-reported`;
    return c.verdict === 'stale' ? `STALE: last check ${when}` : c.verdict === 'missing' ? 'no result yet' : c.verdict;
  };
  const stateTone = { active: 'border-valid text-valid', idle: 'border-line text-neutral-300', no_jobs: 'border-line text-neutral-500' };
  const stateText = (n: Node) => (n.state === 'active' ? 'ACTIVE' : n.state === 'idle' ? `last job ${n.last_job_age_s}s ago` : 'no jobs yet');
</script>

<svelte:head><title>SIMON network: live nodes and receipts</title></svelte:head>

<section class="mx-auto max-w-6xl px-4 py-8">
  <div class="flex flex-wrap items-baseline justify-between gap-3">
    <h1 class="mono text-xl font-bold tracking-wider">Network</h1>
    {#if snap}
      <p class="mono text-xs text-neutral-500">{snap.totals.jobs} jobs · {snap.totals.completion_tokens} tokens · updated {ago(snap.generated_at)}</p>
    {/if}
  </div>
  <p class="mt-2 max-w-3xl text-sm text-neutral-400">
    Live view of SIMON nodes and the jobs they sign. Node state comes only from real jobs. Each receipt's Ed25519
    signature is checked here, in your browser, by the same Rust verifier as the CLI (WebAssembly). Proof level 0:
    a valid signature shows who signed this output, not which model ran.
  </p>

  {#if err && !snap}
    <div class="mt-8 border border-line bg-panel p-6 mono text-sm text-neutral-400">
      No live monitor connected ({err}). This page needs <code>tools/simon-monitor</code> running next to it;
      it never shows sample data.
    </div>
  {:else if !snap}
    <div class="mt-8 mono text-sm text-neutral-500">connecting…</div>
  {:else}
    {#if err}<p class="mt-4 mono text-xs text-bad">monitor unreachable, showing the last data: {err}</p>{/if}

    <div class="mt-6 grid gap-4 md:grid-cols-2">
      {#each snap.nodes as n}
        <article class="scanlines border border-line bg-panel p-5">
          <header class="flex flex-wrap items-center justify-between gap-2">
            <div>
              <h2 class="mono font-bold text-neutral-100">{n.label}</h2>
              <p class="mono text-xs text-neutral-500">model {n.model ?? '–'}</p>
            </div>
            <span class="mono border px-2 py-1 text-xs {stateTone[n.state]}">{stateText(n)}</span>
          </header>

          {#if n.gpu}
            <p class="mt-3 mono text-xs text-neutral-500">{n.gpu.name}{#if n.shared_with} · GPU shared with {n.shared_with}: load is not all SIMON's{/if}</p>
            <div class="mt-3 grid grid-cols-2 gap-4">
              <div><p class="mono text-xs text-neutral-500">GPU load</p><p class="mono text-2xl">{fmt(n.gpu.util)}<span class="text-sm text-neutral-500"> %</span></p><Sparkline values={n.history.util} max={100} /></div>
              <div><p class="mono text-xs text-neutral-500">VRAM</p><p class="mono text-2xl">{fmt(n.gpu.vram_used_mb == null ? null : n.gpu.vram_used_mb / 1024, 1)}<span class="text-sm text-neutral-500"> / {fmt(n.gpu.vram_total_mb == null ? null : n.gpu.vram_total_mb / 1024, 0)} GB</span></p><Sparkline values={n.history.vram} max={100} /></div>
              <div><p class="mono text-xs text-neutral-500">Temp</p><p class="mono text-2xl">{fmt(n.gpu.temp_c)}<span class="text-sm text-neutral-500"> °C</span></p><Sparkline values={n.history.temp} max={100} color="#f59e0b" /></div>
              <div><p class="mono text-xs text-neutral-500">Power</p><p class="mono text-2xl">{fmt(n.gpu.power_w)}<span class="text-sm text-neutral-500"> W / {fmt(n.gpu.power_limit_w)}</span></p><Sparkline values={n.history.power} max={n.gpu.power_limit_w} color="#38bdf8" /></div>
            </div>
          {:else}
            <p class="mt-3 mono text-xs text-neutral-500">{n.telemetry === 'remote' ? 'no GPU telemetry for this node (remote)' : 'GPU telemetry unavailable (nvidia-smi gave no data)'}</p>
          {/if}
          {#if n.canary}
            <p class="mt-3 mono text-xs {canaryTone(n.canary.verdict)}" title="Operator self-check, reported by the node itself: 8 prompts with one-word answers; catches catastrophic weight corruption (e.g. a flipped exponent bit on a GPU without ECC), not subtle damage and not a lying node.">
              integrity canary: {canaryText(n.canary)}
            </p>
          {/if}
          <div class="mt-4">
            <p class="mono text-xs text-neutral-500">tok/s per job (timed by the gateway; includes job setup and time queued on the node)</p>
            <Sparkline values={n.history.tok_s} />
          </div>
        </article>
      {/each}
    </div>

    <h2 class="mt-10 mono text-sm font-bold tracking-wider text-neutral-300">Jobs</h2>
    {#if snap.jobs.length === 0}
      <p class="mt-3 mono text-sm text-neutral-500">No jobs yet.</p>
    {:else}
      <div class="mt-3 overflow-x-auto border border-line">
        <table class="w-full mono text-xs">
          <thead class="bg-panel text-left text-neutral-500">
            <tr><th class="p-2">when</th><th class="p-2">node</th><th class="p-2">job</th><th class="p-2 text-right">tokens</th><th class="p-2 text-right">tok/s</th><th class="p-2">signed by</th><th class="p-2">receipt signature</th></tr>
          </thead>
          <tbody>
            {#each snap.jobs as j (j.job_id)}
              <tr class="border-t border-line">
                <td class="p-2 text-neutral-400">{ago(j.ts)}</td>
                <td class="p-2">{j.node_label ?? j.model}</td>
                <td class="p-2 text-neutral-400">{j.job_id}</td>
                <td class="p-2 text-right">{j.completion_tokens ?? '–'}</td>
                <td class="p-2 text-right">{fmt(j.tok_s, 1)}</td>
                <td class="p-2 text-neutral-400" title={j.node_id}>{short(j.node_id)}</td>
                <td class="p-2">
                  {#if !sig[j.job_id]}<span class="text-neutral-500">checking…</span>
                  {:else if sig[j.job_id].ok}<span class="text-valid">signature valid</span> <span class="text-neutral-500">· output not checked</span>
                  {:else}<span class="text-bad" title={sig[j.job_id].why ?? ''}>signature invalid</span>{/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <p class="mt-2 text-xs text-neutral-500">The answers are not published here, so this checks the signature over the whole receipt, not that a given answer matches it. To check an answer, use the <a class="underline" href="{base}/demo">demo</a> or <code>simon verify</code>.</p>
    {/if}
  {/if}
</section>
