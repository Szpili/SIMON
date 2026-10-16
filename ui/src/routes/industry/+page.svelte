<script lang="ts">
  import { onMount } from 'svelte';
  import { base } from '$app/paths';
  import { verifyReceipt } from '$lib/verify';
  import type { Verdict } from '$lib/receipt';
  import run from '$lib/industry-run.json';
  import { plantNode } from '$lib/plant-nodes';

  // A stored run (tools/simon-industry): every ticket is a real SIMON job; its receipt and the exact
  // signed output are re-checked here by the same Rust verifier as the CLI (WebAssembly).
  type Finding = { mode: string; name: string; rule: string; measured: string; urgency: string };
  type Rec = (typeof run.records)[number];

  interface CapabilityGrant {
    id: string;
    action: string;
    domain: string;
    params_digest: string;
    issuer: string;
    valid_until_us: number;
    budget: number;
    signature: null; // a real grant is signed by the operator's key outside the browser (simon-core cap.rs)
    status: 'UNSIGNED_PREVIEW';
    gated_action: {
      action_code: string;
      part_id: string;
      urgency: string;
      failure_modes: string[];
      checks: string[];
    };
  }

  const s = run.summary;
  // A ticket with any failed check is held for a person, never published (stress test, 2026-10-15).
  const tickets = run.records.filter((r) => r.ticket && !r.problems?.length);
  const held = run.records.filter((r) => r.ticket && r.problems?.length);
  const misses = run.records.filter((r) => !r.ticket && !r.error);
  let verdict = $state<Record<string, { ok: boolean; why: string | null }>>({});

  // Interactive Live Cryptographic Audit & M7 Capability Grant Explorer State
  let selectedRecordId = $state<string>(tickets[0]?.record ?? run.records[0]?.record ?? '');
  let activeRecord = $derived(
    run.records.find((r) => r.record === selectedRecordId) ?? tickets[0] ?? run.records[0]
  );

  let simOutput = $state<string>('');
  let simReceipt = $state<any>(null);
  let simTamperMode = $state<'recorded' | 'mitm' | 'sig_forged'>('recorded');
  let simVerdict = $state<Verdict | null>(null);
  let simVerdictLoading = $state<boolean>(false);
  let grantIssued = $state<CapabilityGrant | null>(null);
  let grantDigest = $state<string | null>(null);

  function flipHex(sig: string): string {
    if (!sig || sig.length < 2) return sig;
    const flipChar = (c: string) => (c.toLowerCase() === '0' ? 'f' : '0');
    return flipChar(sig[0]) + flipChar(sig[1]) + sig.slice(2);
  }

  function canonicalJson(obj: any): string {
    if (obj === null || typeof obj !== 'object') {
      return JSON.stringify(obj);
    }
    if (Array.isArray(obj)) {
      return '[' + obj.map(canonicalJson).join(',') + ']';
    }
    const keys = Object.keys(obj).sort();
    return '{' + keys.map((k) => JSON.stringify(k) + ':' + canonicalJson(obj[k])).join(',') + '}';
  }

  async function computeGrantParamsDigest(action: string, params: Record<string, any>): Promise<string> {
    const canonStr = canonicalJson({
      akcja: action,
      domena: 'SIMON/GRANT-PARAMS/v1',
      parametry: params
    });
    const enc = new TextEncoder();
    const prefix = enc.encode('SIMON/GRANT-PARAMS/v1\0');
    const canonBytes = enc.encode(canonStr);
    const payload = new Uint8Array(prefix.length + canonBytes.length);
    payload.set(prefix, 0);
    payload.set(canonBytes, prefix.length);
    const hashBuf = await crypto.subtle.digest('SHA-256', payload);
    return Array.from(new Uint8Array(hashBuf))
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  }

  async function runSimVerification() {
    if (!simReceipt || !simOutput) return;
    simVerdictLoading = true;
    try {
      const v = await verifyReceipt(JSON.stringify(simReceipt), simOutput);
      simVerdict = v;
    } catch (e) {
      simVerdict = {
        parsuje_sie: false,
        podpis_ok: false,
        tresc_ok: false,
        job_id_ok: false,
        model_ok: false,
        ok: false,
        job_id: null,
        node_id: null,
        model_hash: null,
        runtime: null,
        signer: null,
        output_digest: null,
        powod: String(e)
      };
    } finally {
      simVerdictLoading = false;
    }
  }

  function selectRecord(recId: string) {
    selectedRecordId = recId;
    const r = run.records.find((rec) => rec.record === recId);
    if (!r) return;
    simOutput = r.signed_output ?? '';
    simReceipt = r.receipt ? JSON.parse(JSON.stringify(r.receipt)) : null;
    simTamperMode = 'recorded';
    grantIssued = null;
    grantDigest = null;
    runSimVerification();
  }

  // Button 1: Verify Receipt & Digest
  function handleVerifyRecorded() {
    if (!activeRecord) return;
    simOutput = activeRecord.signed_output ?? '';
    simReceipt = activeRecord.receipt ? JSON.parse(JSON.stringify(activeRecord.receipt)) : null;
    simTamperMode = 'recorded';
    grantIssued = null;
    runSimVerification();
  }

  // Button 2: Simulate MITM Tamper (alter urgency / evidence / needs_human_review)
  function handleSimulateMitmTamper() {
    if (!simOutput) return;
    if (simOutput.includes('"needs_human_review": true')) {
      simOutput = simOutput.replace('"needs_human_review": true', '"needs_human_review": false');
    } else if (simOutput.includes('"urgency"')) {
      simOutput = simOutput.replace(/"urgency":\s*"[^"]+"/, '"urgency": "unauthorized-urgent-override"');
    } else {
      simOutput = simOutput + ' ';
    }
    simTamperMode = 'mitm';
    runSimVerification();
  }

  // Button 3: Simulate Signature Forgery (flip Ed25519 byte)
  function handleSimulateSigForgery() {
    if (!simReceipt || !simReceipt.signature) return;
    simReceipt.signature = flipHex(simReceipt.signature);
    simTamperMode = 'sig_forged';
    runSimVerification();
  }

  // Button 4: Issue M7 Operator Capability Grant
  async function handleIssueGrant() {
    if (!activeRecord || !activeRecord.ticket) return;
    const t = activeRecord.ticket;
    const mode = t.failure_modes?.[0] ?? 'HDF';
    const actionCodes: Record<string, string> = {
      HDF: 'INSPECT_COOLING',
      PWF: 'CALIBRATE_POWER_DRIVE',
      OSF: 'REDUCE_FEED_STRAIN',
      TWF: 'REPLACE_TOOL_INSERT'
    };
    const partIds: Record<string, string> = {
      HDF: 'FAN-CLR-8020',
      PWF: 'DRV-INV-440',
      OSF: 'CPL-TRQ-250',
      TWF: 'INS-CRB-100'
    };

    const actionCode = actionCodes[mode] ?? 'INSPECT_COOLING';
    const partId = partIds[mode] ?? 'FAN-CLR-8020';

    const params = {
      action_code: actionCode,
      failure_modes: t.failure_modes ?? [mode],
      part_id: partId,
      summary: t.summary ?? '',
      urgency: t.urgency ?? 'inspect before next shift'
    };

    const digestHex = await computeGrantParamsDigest('propose_work_order', params);
    grantDigest = digestHex;

    const nowUs = Date.now() * 1000;
    const nonce = Math.random().toString(16).slice(2, 10);

    grantIssued = {
      id: `grant-${activeRecord.record}-${nonce}`,
      action: 'propose_work_order',
      domain: 'SIMON/GRANT-PARAMS/v1',
      params_digest: digestHex,
      issuer: 'operator key (never in the browser)',
      valid_until_us: nowUs + 3600 * 1000 * 1000,
      budget: 1,
      // No fake signature: the browser has no operator key. The digest above is real; the grant is a preview.
      signature: null,
      status: 'UNSIGNED_PREVIEW',
      gated_action: {
        action_code: actionCode,
        part_id: partId,
        urgency: params.urgency,
        failure_modes: params.failure_modes,
        checks: t.checks ?? [] // shown for the technician; NOT part of the 5 hashed parameters
      }
    };
  }

  onMount(() => {
    for (const r of [...tickets, ...held]) {
      if (!r.receipt || !r.signed_output) continue;
      verifyReceipt(JSON.stringify(r.receipt), r.signed_output)
        .then((v) => (verdict[r.record] = { ok: v.ok, why: v.powod }))
        .catch((e) => (verdict[r.record] = { ok: false, why: String(e) }));
    }
    if (activeRecord) {
      selectRecord(activeRecord.record);
    }
  });

  const short = (x: string | null | undefined) => (x ? `${x.slice(0, 8)}…${x.slice(-6)}` : '–');
  const modes = (r: Rec) => (r.findings as Finding[]).map((f) => f.mode).join(' + ');
  const failed = (r: Rec) =>
    Object.entries(r.label).filter(([, v]) => v).map(([k]) => k).join(', ') || 'none labelled';
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
    <li class="border border-line bg-panel p-3">
      <b class="text-neutral-100">1 · Rules decide</b><br />
      Deterministic physics thresholds flag the risk and the failure mode. The language model never decides whether a machine is at risk.
    </li>
    <li class="border border-line bg-panel p-3">
      <b class="text-neutral-100">2 · Gemma drafts & triages</b><br />
      Gemma 4 26B-A4B writes the title and summary and picks inspection steps by ID from a fixed catalog per failure mode. It never writes a step itself.
    </li>
    <li class="border border-line bg-panel p-3">
      <b class="text-neutral-100">3 · M7 Capability Ceiling</b><br />
      Exact fields, steps from catalog, no shutdown instructions. Actions gated pending an Ed25519 operator capability grant token.
    </li>
    <li class="border border-line bg-panel p-3">
      <b class="text-neutral-100">4 · Signed Receipt</b><br />
      The SIMON node signs the exact ticket text. Re-verified below in your browser offline via WebAssembly.
    </li>
  </ol>

  <div class="mt-6 grid gap-3 sm:grid-cols-4 mono">
    <div class="border border-line bg-panel p-4"><p class="text-xs text-neutral-500">published (passed every check)</p><p class="text-2xl">{s.clean}/{s.tickets}</p></div>
    <div class="border border-line bg-panel p-4"><p class="text-xs text-neutral-500">median time per ticket</p><p class="text-2xl">{(s.median_ms / 1000).toFixed(1)} s</p></div>
    <div class="border border-line bg-panel p-4"><p class="text-xs text-neutral-500">records in the predeclared sample</p><p class="text-2xl">{s.records}</p></div>
    <div class="border border-line bg-panel p-4"><p class="text-xs text-neutral-500">GPU nodes in this run</p>{#each s.nodes as n}<p class="mt-1 text-xs leading-snug text-neutral-200">{n}</p>{/each}</div>
  </div>

  <!-- Live Cryptographic Audit & M7 Capability Grant Explorer -->
  <div class="mt-10 border-2 border-line bg-panel p-5 shadow-2xl">
    <div class="flex flex-wrap items-center justify-between gap-4 border-b border-line pb-4">
      <div>
        <div class="flex items-center gap-2">
          <span
            aria-hidden="true"
            class="inline-block h-3 w-3 shrink-0 rounded-full {simVerdict?.ok ? 'bg-valid shadow-[0_0_8px_#22c55e]' : 'bg-bad shadow-[0_0_8px_#ef4444]'}"
          ></span>
          <h2 class="mono text-base font-bold tracking-wider text-neutral-100">
            Live Cryptographic Audit & M7 Capability Grant Explorer
          </h2>
        </div>
        <p class="mt-1 text-xs text-neutral-400">
          Selected Record: <span class="mono font-bold text-amber-400">#{activeRecord.record}</span> · Product {activeRecord.product} · Node: <span class="text-neutral-200">{activeRecord.node_label ?? short(activeRecord.node_id)}</span>
        </p>
      </div>

      <!-- Quick Record Switcher -->
      <div class="flex items-center gap-2 mono text-xs">
        <label for="record-select" class="text-neutral-400">Inspect record:</label>
        <select
          id="record-select"
          class="border border-line bg-neutral-900 px-2 py-1 text-neutral-200 focus:border-valid focus:outline-none"
          value={selectedRecordId}
          onchange={(e) => selectRecord((e.target as HTMLSelectElement).value)}
        >
          {#each tickets as r}
            <option value={r.record}>Ticket #{r.record} ({modes(r)})</option>
          {/each}
          {#each held as r}
            <option value={r.record}>Held #{r.record} ({modes(r)})</option>
          {/each}
        </select>
      </div>
    </div>

    <!-- Action Buttons Row -->
    <div class="mt-4 flex flex-wrap items-center gap-2 mono text-xs">
      <button
        type="button"
        aria-pressed={simTamperMode === 'recorded' && !grantIssued}
        class="border px-3 py-1.5 text-left transition-colors {simTamperMode === 'recorded' && !grantIssued ? 'border-valid bg-valid/10 text-valid' : 'border-line bg-neutral-900 text-neutral-300 hover:border-neutral-400'}"
        onclick={handleVerifyRecorded}
      >
        1 — Verify Receipt & Digest (WASM Ed25519 + SHA-256)
      </button>

      <button
        type="button"
        aria-pressed={simTamperMode === 'mitm'}
        class="border px-3 py-1.5 text-left transition-colors {simTamperMode === 'mitm' ? 'border-bad bg-bad/10 text-bad font-bold' : 'border-line bg-neutral-900 text-neutral-300 hover:border-neutral-400'}"
        onclick={handleSimulateMitmTamper}
      >
        2 — Simulate Text Tamper (Alter Urgency / Review Flag)
      </button>

      <button
        type="button"
        aria-pressed={simTamperMode === 'sig_forged'}
        class="border px-3 py-1.5 text-left transition-colors {simTamperMode === 'sig_forged' ? 'border-bad bg-bad/10 text-bad font-bold' : 'border-line bg-neutral-900 text-neutral-300 hover:border-neutral-400'}"
        onclick={handleSimulateSigForgery}
      >
        3 — Simulate Signature Tamper (Flip Ed25519 Byte)
      </button>

      <button
        type="button"
        aria-pressed={Boolean(grantIssued)}
        class="border px-3 py-1.5 text-left transition-colors {grantIssued ? 'border-amber-400 bg-amber-400/10 text-amber-300 font-bold' : 'border-line bg-neutral-900 text-neutral-300 hover:border-amber-400'}"
        onclick={handleIssueGrant}
      >
        4 — Preview Operator Grant (unsigned)
      </button>
    </div>

    <!-- Live WASM & Cryptographic Gate Verdict -->
    <div class="mt-5 grid gap-4 lg:grid-cols-2">
      <!-- Left: Ticket Text & Tamper State -->
      <div class="min-w-0 border border-line bg-neutral-950 p-4 mono text-xs">
        <div class="flex flex-wrap items-center justify-between gap-2 border-b border-line pb-2">
          <span class="text-neutral-400">Signed Output Text (WASM Input):</span>
          {#if simTamperMode === 'recorded'}
            <span class="text-valid">Matches signed digest</span>
          {:else if simTamperMode === 'mitm'}
            <span class="text-bad font-bold">TEXT ALTERED (Digest Mismatch)</span>
          {:else if simTamperMode === 'sig_forged'}
            <span class="text-bad font-bold">SIGNATURE ALTERED (Byte Flipped)</span>
          {/if}
        </div>
        <pre class="mt-3 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-neutral-900 p-2.5 text-neutral-300 leading-relaxed">{simOutput}</pre>

        <div class="mt-3 text-neutral-400 text-[11px] space-y-1">
          <p class="break-all">Node Signer Key: <span class="text-neutral-300">{simReceipt?.signer ?? '–'}</span></p>
          <p>Signer on plant node list: {#if plantNode(simReceipt?.signer)}<span class="text-valid">yes · {plantNode(simReceipt?.signer)}</span>{:else}<span class="text-amber-300">no · unknown node</span>{/if} <a class="underline text-neutral-400 hover:text-neutral-200" href="{base}/receipts">what a receipt proves</a></p>
          <p>Signature (Ed25519): <span class="text-neutral-300">{short(simReceipt?.signature)}</span></p>
          <p>Claimed Model (unverified): <span class="text-neutral-300">{simReceipt?.model_hash ?? '–'}</span></p>
        </div>
      </div>

      <!-- Right: Live WASM Gate Verdict Details -->
      <div class="min-w-0 border border-line bg-neutral-950 p-4 mono text-xs" aria-live="polite">
        <div class="flex flex-wrap items-center justify-between gap-2 border-b border-line pb-2">
          <span class="text-neutral-400">WASM Offline Gate Verification:</span>
          {#if simVerdictLoading}
            <span class="text-neutral-400">verifying via WebAssembly…</span>
          {:else if simVerdict?.ok}
            <span class="text-valid font-bold">Signed by {short(simVerdict.node_id)} · claimed model: {simVerdict.model_hash}</span>
          {:else}
            <span class="text-bad font-bold">REJECTED — Cryptographic verification failed</span>
          {/if}
        </div>

        <div class="mt-3 space-y-2">
          <div class="flex flex-wrap items-center justify-between gap-2 border-b border-line/40 py-1">
            <span class="text-neutral-400">JSON Parse Gate:</span>
            <span class={simVerdict?.parsuje_sie ? 'text-valid' : 'text-bad'}>{simVerdict?.parsuje_sie ? 'PASSED' : 'FAILED'}</span>
          </div>
          <div class="flex flex-wrap items-center justify-between gap-2 border-b border-line/40 py-1">
            <span class="text-neutral-400">Ed25519 Signature Gate (signature_ok):</span>
            <span class={simVerdict?.podpis_ok ? 'text-valid' : 'text-bad'}>
              {simVerdict?.podpis_ok ? 'VALID (Signed by Node Keypair)' : 'FAILED (Signature Forgery Blocked)'}
            </span>
          </div>
          <div class="flex flex-wrap items-center justify-between gap-2 border-b border-line/40 py-1">
            <span class="text-neutral-400">Output Digest Gate (SIMON/OUTPUT/v1 digest_ok):</span>
            <span class={simVerdict?.tresc_ok ? 'text-valid' : 'text-bad'}>
              {simVerdict?.tresc_ok ? 'VALID (SHA-256 Digest Matches)' : 'FAILED (text differs from signed digest)'}
            </span>
          </div>
          <div class="flex flex-wrap items-center justify-between gap-2 border-b border-line/40 py-1">
            <span class="text-neutral-400">Overall Gate Verdict:</span>
            <span class="font-bold {simVerdict?.ok ? 'text-valid' : 'text-bad'}">
              {simVerdict?.ok ? 'SIGNATURE & DIGEST VALID' : `DENY TICKET: ${simVerdict?.powod ?? 'Verification failed'}`}
            </span>
          </div>
        </div>

        <p class="mt-2 text-[11px] text-neutral-400">
          Proves signer key and exact text; does not prove which model weights executed.
        </p>

        {#if simTamperMode === 'mitm'}
          <div class="mt-3 border border-bad/50 bg-bad/10 p-2 text-bad text-[11px]">
            <b>Text Modification Detected:</b> Text differs from signed digest. The offline Rust WASM verifier rejected the ticket because the computed SHA-256 does not match the receipt's signed digest.
          </div>
        {:else if simTamperMode === 'sig_forged'}
          <div class="mt-3 border border-bad/50 bg-bad/10 p-2 text-bad text-[11px]">
            <b>Signature Forgery Detected:</b> Ed25519 signature byte flipped. The mathematical signature check failed offline without calling any external authority.
          </div>
        {/if}
      </div>
    </div>

    <!-- M7 Capability Grant Panel (Single-Use Capability Token) -->
    {#if grantIssued}
      <div class="mt-5 border border-amber-400/80 bg-neutral-950 p-4 mono text-xs shadow-lg">
        <div class="flex flex-wrap items-center justify-between gap-2 border-b border-amber-400/30 pb-2">
          <div class="flex items-center gap-2">
            <span aria-hidden="true" class="inline-block h-2.5 w-2.5 rounded-full bg-amber-400 animate-pulse"></span>
            <h3 class="font-bold text-amber-300">
              Operator grant preview (unsigned)
            </h3>
          </div>
          <span class="border border-amber-400/60 bg-amber-400/10 px-2 py-0.5 text-amber-300 text-[11px]">
            needs_human_review: true
          </span>
        </div>

        <p class="mt-2 text-neutral-300 text-xs">
          <b>Capability ceiling:</b> in the SIMON harness a gated action runs only with a single-use grant that the operator signs for exactly these parameters. This page has no operator key, so it shows the parameters and their digest, not a signed grant.
        </p>

        <div class="mt-3 grid gap-3 md:grid-cols-2">
          <div class="min-w-0">
            <p class="text-neutral-400 text-[11px]">Proposed work order parameters:</p>
            <ul class="mt-1 space-y-1 text-neutral-200 text-xs bg-neutral-900 p-2.5 rounded border border-line">
              <li>Action Code: <span class="text-amber-400 font-bold">{grantIssued.gated_action.action_code}</span></li>
              <li>Part ID: <span class="text-amber-400">{grantIssued.gated_action.part_id}</span></li>
              <li>Urgency: <span class="text-neutral-300">{grantIssued.gated_action.urgency}</span></li>
              <li>Failure Modes: <span class="text-neutral-300">{grantIssued.gated_action.failure_modes.join(', ')}</span></li>
              <li>Catalog Checks: <span class="text-neutral-400">{grantIssued.gated_action.checks.join(', ')}</span></li>
            </ul>
          </div>

          <div class="min-w-0">
            <p class="text-neutral-400 text-[11px]">Exact SIMON/GRANT-PARAMS/v1 Parameter Commitment (WebCrypto SHA-256):</p>
            <div class="mt-1 bg-neutral-900 p-2.5 rounded border border-line font-mono text-[11px] break-all">
              <p class="text-amber-300">{grantIssued.params_digest}</p>
              <p class="mt-1.5 text-neutral-400">
                Same digest as <code>crates/simon-core/src/cap.rs</code> (<code>odcisk_parametrow_akcji</code>). An operator grant signs this digest; changing any parameter breaks it.
              </p>
            </div>
          </div>
        </div>

        <pre class="mt-3 max-h-36 overflow-auto whitespace-pre-wrap break-all rounded bg-neutral-900 p-2 text-neutral-400 text-[10px]">{JSON.stringify(grantIssued, null, 2)}</pre>
      </div>
    {/if}
  </div>

  <h2 class="mt-10 mono text-sm font-bold tracking-wider text-neutral-300">Tickets</h2>
  <div class="mt-3 grid gap-4 md:grid-cols-2">
    {#each tickets as r (r.record)}
      <article
        class="min-w-0 border border-line bg-panel p-4 text-sm transition-all hover:border-neutral-400 {selectedRecordId === r.record ? 'ring-2 ring-amber-400/80 border-amber-400' : ''}"
      >
        <header class="flex flex-wrap items-baseline justify-between gap-2 mono text-xs text-neutral-400">
          <div class="flex items-center gap-1.5 flex-wrap">
            <span>record {r.record} · {r.product} · rules: <span class="text-amber-400">{modes(r)}</span></span>
            <button
              type="button"
              aria-pressed={selectedRecordId === r.record}
              aria-label="Inspect record {r.record} in explorer"
              class="border px-2 py-0.5 text-[10px] transition-colors {selectedRecordId === r.record ? 'border-amber-400 bg-amber-400/20 text-amber-300 font-bold' : 'border-line text-neutral-400 hover:border-neutral-300 hover:text-neutral-200'}"
              onclick={() => selectRecord(r.record)}
            >
              {selectedRecordId === r.record ? '★ Active in Explorer' : 'Inspect in Explorer →'}
            </button>
          </div>
          {#if !verdict[r.record]}<span>checking receipt…</span>
          {:else if verdict[r.record].ok}<span class="text-valid">RECEIPT VALID · signature + text</span>
            {#if plantNode(r.receipt?.signer)}<span class="border border-valid/60 px-1.5 text-valid">plant-listed node: {plantNode(r.receipt?.signer)}</span>
            {:else}<span class="border border-amber-400/60 px-1.5 text-amber-300" title="Valid signature from a key that is not on the plant's node list">unknown node</span>{/if}
          {:else}<span class="text-bad" title={verdict[r.record].why ?? ''}>RECEIPT REJECTED</span>{/if}
        </header>
        <h3 class="mt-2 font-bold text-neutral-100">{r.ticket?.title}</h3>
        <p class="mt-1 text-neutral-300">{r.ticket?.summary}</p>
        <ul class="mt-2 list-disc pl-5 text-neutral-300">{#each r.ticket?.check_steps ?? [] as c, i}<li>{c} <span class="mono whitespace-nowrap text-xs text-neutral-500">{r.ticket?.checks?.[i]}</span></li>{/each}</ul>
        <p class="mt-2 mono text-xs text-neutral-500">urgency: {r.ticket?.urgency} · evidence: {(r.ticket?.evidence ?? []).join(', ')}</p>
        <p class="mt-1 mono text-xs text-neutral-500">signed by {short(r.node_id)} ({r.node_label ?? 'node'}) · model {r.receipt?.model_hash} · {r.wall_ms} ms</p>
      </article>
    {/each}
  </div>

  {#if held.length}
    <h2 class="mt-10 mono text-sm font-bold tracking-wider text-neutral-300">Held for review (not published)</h2>
    <p class="mt-2 text-sm text-neutral-400">These drafts failed a check, so they go to a person instead of the technician. The signed draft stays in the audit trail.</p>
    {#each held as r (r.record)}
      <article
        class="mt-3 min-w-0 border border-bad/60 bg-panel p-4 text-sm transition-all hover:border-bad {selectedRecordId === r.record ? 'ring-2 ring-bad' : ''}"
      >
        <header class="flex flex-wrap items-baseline justify-between gap-2 mono text-xs text-neutral-400">
          <div class="flex items-center gap-1.5 flex-wrap">
            <span>record {r.record} · {r.product} · rules: <span class="text-amber-400">{modes(r)}</span></span>
            <button
              type="button"
              aria-pressed={selectedRecordId === r.record}
              aria-label="Inspect held record {r.record} in explorer"
              class="border px-2 py-0.5 text-[10px] transition-colors {selectedRecordId === r.record ? 'border-bad bg-bad/20 text-bad font-bold' : 'border-line text-neutral-400 hover:border-bad hover:text-bad'}"
              onclick={() => selectRecord(r.record)}
            >
              {selectedRecordId === r.record ? '★ Active in Explorer' : 'Inspect in Explorer →'}
            </button>
          </div>
          {#if !verdict[r.record]}<span>checking receipt…</span>
          {:else if verdict[r.record].ok}<span class="text-valid">RECEIPT VALID · signature + text</span>
            {#if plantNode(r.receipt?.signer)}<span class="border border-valid/60 px-1.5 text-valid">plant-listed node: {plantNode(r.receipt?.signer)}</span>
            {:else}<span class="border border-amber-400/60 px-1.5 text-amber-300" title="Valid signature from a key that is not on the plant's node list">unknown node</span>{/if}
          {:else}<span class="text-bad" title={verdict[r.record].why ?? ''}>RECEIPT REJECTED</span>{/if}
        </header>
        <p class="mt-2 mono text-xs text-bad">held: {r.problems?.join('; ')}</p>
        <p class="mt-1 text-neutral-400">draft summary: “{r.ticket?.summary}”</p>
        <p class="mt-1 mono text-xs text-neutral-500">signed by {short(r.node_id)} ({r.node_label ?? 'node'}) · rules said: {r.findings.map((f) => f.urgency).join(', ')}</p>
      </article>
    {/each}
  {/if}

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
    <li>Gemma's role is the ticket text, not the diagnosis. Inspection steps come only from the plant's catalog; evidence is copied from the rules by code. What the checks cannot judge is the meaning of the short free-text summary, so a technician reviews every ticket. The stress cases, including that known limit, run in CI: <code>tools/simon-industry/stress_corpus.json</code>.</li>
    <li>A receipt (proof level 0) proves which node's key signed this exact ticket. It does not prove which model ran; see the <a class="underline" href="{base}/demo">demo</a> and <a class="underline" href="{base}/network">network</a> pages.</li>
    <li>The sample (18 records, fixed seed, stratified by rule outcome) was chosen in code before any model output was seen: <code>tools/simon-industry/tickets.py</code>.</li>
  </ul>
</section>
