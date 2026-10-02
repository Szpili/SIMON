<script lang="ts">
  import '../app.css';
  import { base } from '$app/paths';
  let { children } = $props();
</script>

<div class="flex min-h-screen flex-col bg-ink">
  <header class="border-b border-line">
    <nav class="mx-auto flex max-w-6xl items-center justify-between px-4 py-3 mono text-sm">
      <a href="{base}/" class="font-bold tracking-widest text-neutral-100">SIMON</a>
      <div class="flex gap-5 text-neutral-400">
        <a class="hover:text-neutral-100" href="{base}/demo">demo</a>
        <a class="hover:text-neutral-100" href="https://github.com/Szpili/SIMON" rel="noopener">source</a>
      </div>
    </nav>
  </header>
  <main class="flex-1">{@render children()}</main>
  <footer class="border-t border-line py-4 text-center mono text-xs text-neutral-500">
    No analytics. No tracking. Verification runs in your browser.
  </footer>
</div>
