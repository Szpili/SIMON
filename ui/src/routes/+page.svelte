<script lang="ts">
  import { base } from '$app/paths';
  const logo = `  ███████╗██╗███╗   ███╗ ██████╗ ███╗   ██╗
  ██╔════╝██║████╗ ████║██╔═══██╗████╗  ██║
  ███████╗██║██╔████╔██║██║   ██║██╔██╗ ██║
  ╚════██║██║██║╚██╔╝██║██║   ██║██║╚██╗██║
  ███████║██║██║ ╚═╝ ██║╚██████╔╝██║ ╚████║
  ╚══════╝╚═╝╚═╝     ╚═╝ ╚═════╝ ╚═╝  ╚═══╝`;
  const chips = ['signed', 'verified offline', 'no server'];
</script>

<svelte:head><title>SIMON: verify inference receipts offline</title></svelte:head>

<section class="mx-auto flex max-w-3xl flex-col items-start px-4 py-16 sm:py-24">
  <pre class="mono max-w-full overflow-x-auto text-[9px] leading-[1.15] text-valid sm:text-sm sm:leading-[1.15]" aria-label="SIMON">{logo}</pre>
  <p class="mono mt-4 text-sm tracking-wider text-neutral-400">Sovereign Inference &amp; Model Orchestration Network</p>
  <p class="mt-2 text-lg font-bold text-neutral-100">Your models. Independent nodes. Inference on demand.</p>

  <p class="mt-10 max-w-xl text-xl leading-relaxed text-neutral-100 sm:text-2xl">
    Every inference job produces an Ed25519-signed receipt bound to its output. Anyone can verify it offline — in the browser, with the same Rust verifier the node harness uses.
  </p>

  <ul class="mt-6 flex flex-wrap gap-2">
    {#each chips as c}
      <li class="mono border border-line bg-panel px-3 py-1 text-xs text-neutral-300">{c}</li>
    {/each}
  </ul>

  <div class="mt-10 flex flex-wrap gap-3">
    <a href="{base}/industry" class="btn btn-primary px-6 py-3 text-base">Shop-floor demo: Gemma 4 on AMD MI300X</a>
    <a href="{base}/demo" class="btn px-6 py-3 text-base">Tamper with a receipt</a>
  </div>
  <p class="mt-4 max-w-md text-sm text-neutral-500">Maintenance tickets written by Gemma 4 on the plant's RTX 3090 and an AMD Instinct MI300X, each with a signed receipt checked in your browser. Or tamper with a node's output and watch verification fail.</p>
</section>
