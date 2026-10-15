<script lang="ts">
  import { onMount } from 'svelte';
  import { base } from '$app/paths';
  import { verifyReceipt } from '$lib/verify';
  import run from '$lib/industry-run.json';

  // A stored run (tools/simon-industry): every ticket is a real SIMON job; its receipt and the exact
  // signed output are re-checked here by the same Rust verifier as the CLI (WebAssembly).
  type Finding = { mode: string; name: string; rule: string; measured: string; urgency: string };
  type Rec = (typeof run.records)[number];

  const s = run.summary;
  const tickets = run.records.filter((r) => r.ticket);
  const misses = run.records.filter((r) => !r.ticket && !r.error);
  let verdict = $state<Record<string, { ok: boolean; why: string | null }>>({});

  onMount(() => {
    for (const r of tickets) {
      if (!r.receipt || !r.signed_output) continue;
      verifyReceipt(JSON.stringify(r.receipt), r.signed_output)
        .then((v) => (verdict[r.record] = { ok: v.ok, why: v.powod }))
        .catch((e) => (verdict[r.record] = { ok: false, why: String(e) }));
    }
  });

  const short = (x: string | null | undefined) => (x ? `${x.slice(0, 8)}…${x.slice(-6)}` : '–');
  const modes = (r: Rec) => (r.findings as Finding[]).map((f) => f.mode).join(' + ');
  const failed = (r: Rec) => Object.entries(r.label).filter(([, v]) => v).map(([k]) => k).join(', ') || 'none labelled';
</script>

<svelte:head><title>SIMON for the shop floor: maintenance tickets with an audit trail</title></svelte:head>

<section class="mx-auto max-w-6xl px-4 py-8">
  <h1 class="mono text-xl font-bold tracking-wider">Shop floor: maintenance tickets with an audit trail</h1>
  <p class="mt-3 max-w-3xl text-sm text-neutral-300">
    <b>User:</b> the maintenance planner of a machining plant. <b>Problem:</b> machine alarms turn into tickets by
    hand, late, and nobody can later prove which system produced which advice. <b>SIMON:</b> the plant's monitoring
    rules flag a machine; Gemma 4 drafts the technician ticket on the plant's own GPUs or on AMD Instinct cloud
    GPUs; every ticket carries a signed receipt from the GPU node that wrote it.
  </p>

  <ol class="mt-6 grid gap-3 md:grid-cols-4 mono text-xs">
    <li class="border border-line bg-panel p-3"><b class="text-neutral-100">1 · Rules decide</b><br />Deterministic physics thresholds flag the risk and the failure mode. The language model never decides whether a machine is at risk.</li>
    <li class="border border-line bg-panel p-3"><b class="text-neutral-100">2 · Gemma drafts</b><br />Gemma 4 26B-A4B writes the ticket (JSON) from the rule findings only: checks, urgency, evidence, marked for human review.</li>
    <li class="border border-line bg-panel p-3"><b class="text-neutral-100">3 · Checked</b><br />Valid JSON, failure modes equal to the rules, no number the input lacks, no command to stop or bypass equipment.</li>
    <li class="border border-line bg-panel p-3"><b class="text-neutral-100">4 · Signed</b><br />The SIMON node signs the exact ticket text. Verified below, in your browser, offline.</li>
  </ol>

  <div class="mt-6 grid gap-3 sm:grid-cols-4 mono">
    <div class="border border-line bg-panel p-4"><p class="text-xs text-neutral-500">tickets that passed every check</p><p class="text-2xl">{s.clean}/{s.tickets}</p></div>
    <div class="border border-line bg-panel p-4"><p class="text-xs text-neutral-500">median time per ticket</p><p class="text-2xl">{(s.median_ms / 1000).toFixed(1)} s</p></div>
    <div class="border border-line bg-panel p-4"><p class="text-xs text-neutral-500">records in the predeclared sample</p><p class="text-2xl">{s.records}</p></div>
    <div class="border border-line bg-panel p-4"><p class="text-xs text-neutral-500">GPU nodes in this run</p>{#each s.nodes as n}<p class="mt-1 text-xs leading-snug text-neutral-200">{n}</p>{/each}</div>
  </div>

  <h2 class="mt-10 mono text-sm font-bold tracking-wider text-neutral-300">Tickets</h2>
  <div class="mt-3 grid gap-4 md:grid-cols-2">
    {#each tickets as r (r.record)}
      <article class="border border-line bg-panel p-4 text-sm">
        <header class="flex flex-wrap items-baseline justify-between gap-2 mono text-xs text-neutral-500">
          <span>record {r.record} · {r.product} · rules: <span class="text-amber-400">{modes(r)}</span></span>
          {#if !verdict[r.record]}<span>checking receipt…</span>
          {:else if verdict[r.record].ok}<span class="text-valid">RECEIPT VALID · signature + text</span>
          {:else}<span class="text-bad" title={verdict[r.record].why ?? ''}>RECEIPT REJECTED</span>{/if}
        </header>
        <h3 class="mt-2 font-bold text-neutral-100">{r.ticket?.title}</h3>
        <p class="mt-1 text-neutral-300">{r.ticket?.summary}</p>
        <ul class="mt-2 list-disc pl-5 text-neutral-300">{#each r.ticket?.checks ?? [] as c}<li>{c}</li>{/each}</ul>
        <p class="mt-2 mono text-xs text-neutral-500">urgency: {r.ticket?.urgency} · evidence: {(r.ticket?.evidence ?? []).join(', ')}</p>
        <p class="mt-1 mono text-xs text-neutral-500">signed by {short(r.node_id)} ({r.node_label ?? 'node'}) · model {r.receipt?.model_hash} · {r.wall_ms} ms</p>
        {#if r.problems?.length}<p class="mt-1 mono text-xs text-bad">checks failed: {r.problems.join('; ')}</p>{/if}
      </article>
    {/each}
  </div>

  {#if misses.length}
    <h2 class="mt-10 mono text-sm font-bold tracking-wider text-neutral-300">What the rules miss (no ticket)</h2>
    <p class="mt-2 text-sm text-neutral-400">Labelled failures in the sample that no rule flags, so no ticket was written. Shown on purpose: the pipeline is only as good as the plant's rules.</p>
    <ul class="mt-2 mono text-xs text-neutral-400">
      {#each misses as r}<li>record {r.record} · {r.product} · labelled failure: {failed(r)} · {r.features.wear_min} min tool wear, {r.features.power_W} W, Δt {r.features.temp_diff_K} K</li>{/each}
    </ul>
  {/if}

  <h2 class="mt-10 mono text-sm font-bold tracking-wider text-neutral-300">Read this before trusting it</h2>
  <ul class="mt-2 list-disc pl-5 text-sm text-neutral-400">
    <li>Data: AI4I 2020 Predictive Maintenance (UCI, CC BY 4.0): 10,000 synthetic records modelled on a real milling machine. Its failure labels follow published physics, so our rules catch 321 of 339 labelled failures here; that is the dataset's design, not a result to quote for a real plant.</li>
    <li>Gemma's role is the ticket text, not the diagnosis. A technician reviews every ticket.</li>
    <li>A receipt (proof level 0) proves which node's key signed this exact ticket. It does not prove which model ran; see the <a class="underline" href="{base}/demo">demo</a> and <a class="underline" href="{base}/network">network</a> pages.</li>
    <li>The sample (18 records, fixed seed, stratified by rule outcome) was chosen in code before any model output was seen: <code>tools/simon-industry/tickets.py</code>.</li>
  </ul>
</section>
