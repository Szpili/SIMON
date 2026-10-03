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

  <p class="mt-10 max-w-xl text-xl leading-relaxed text-neutral-100 sm:text-2xl">
    Every inference job produces an Ed25519-signed receipt bound to its output. Anyone can verify it offline — in the browser, with the same Rust verifier the node harness uses.
  </p>

  <ul class="mt-6 flex flex-wrap gap-2">
    {#each chips as c}
      <li class="mono border border-line bg-panel px-3 py-1 text-xs text-neutral-300">{c}</li>
    {/each}
  </ul>

  <a href="{base}/demo" class="btn btn-primary mt-10 px-6 py-3 text-base">Try the demo</a>
  <p class="mt-4 max-w-md text-sm text-neutral-500">Tamper with a node's output, then check the receipt. It takes about ten seconds.</p>
</section>
