<script lang="ts">
  import { base } from '$app/paths';
</script>

<svelte:head><title>SIMON: what a receipt proves</title></svelte:head>

<section class="mx-auto max-w-3xl px-4 py-12 text-neutral-200">
  <h1 class="mono text-2xl font-bold tracking-wider">What a receipt proves</h1>
  <p class="mt-4 text-neutral-400">
    Every SIMON job returns an Ed25519-signed receipt. It is checked in your browser by the same Rust verifier
    the command line uses. Here is exactly what a valid receipt does and does not tell you.
  </p>

  <h2 class="mt-8 mono text-sm font-bold tracking-wider text-valid">It proves</h2>
  <ul class="mt-3 list-disc space-y-2 pl-5">
    <li>The holder of one Ed25519 key signed this exact text for this job (the output digest is in the signature).</li>
    <li>Change one character of the text, or one signed field, and verification fails.</li>
    <li>If that key is on the plant's list of nodes (<span class="text-valid">plant-listed node</span>), the text came from that machine's key.</li>
  </ul>

  <h2 class="mt-8 mono text-sm font-bold tracking-wider text-amber-300">It does not prove</h2>
  <ul class="mt-3 list-disc space-y-2 pl-5">
    <li>
      Who the signer is, unless you pin the key. A receipt carries its own public key, so anyone can mint a key and
      sign any text. A valid signature from a key that is not listed shows as <span class="text-amber-300">unknown node</span>.
    </li>
    <li>Which model, quantization or precision ran. The node declares them; it can lie (see the verifier pilot for detection research).</li>
    <li>That the answer is correct or safe. Industrial tickets are also checked against the rules and reviewed by a technician.</li>
  </ul>

  <h2 class="mt-8 mono text-sm font-bold tracking-wider text-neutral-300">How to use it</h2>
  <ul class="mt-3 list-disc space-y-2 pl-5">
    <li>Pin the keys of your own nodes (<code>node_id</code> in the gateway's <code>nodes.json</code>, or <code>simon verify --signer</code>).</li>
    <li>Treat <span class="text-amber-300">unknown node</span> as unverified, even when the signature is valid.</li>
  </ul>

  <p class="mt-10 text-sm text-neutral-500">
    See it on real tickets: <a class="underline hover:text-neutral-200" href="{base}/industry">/industry</a> ·
    tamper with a receipt yourself: <a class="underline hover:text-neutral-200" href="{base}/demo">/demo</a>
  </p>
</section>
