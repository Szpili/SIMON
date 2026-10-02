<script lang="ts">
  let { label, value, head = 10, tail = 6 }: { label: string; value: string; head?: number; tail?: number } = $props();
  let copied = $state(false);
  const short = $derived(value.length > head + tail + 1 ? value.slice(0, head) + '…' + value.slice(-tail) : value);

  async function copy() {
    try { await navigator.clipboard.writeText(value); }
    catch {
      const t = document.createElement('textarea');
      t.value = value; document.body.appendChild(t); t.select();
      try { document.execCommand('copy'); } finally { t.remove(); }
    }
    copied = true; setTimeout(() => (copied = false), 1200);
  }
</script>

<div class="flex items-center justify-between gap-3 border-b border-line py-2 last:border-b-0">
  <span class="mono shrink-0 text-xs text-neutral-500">{label}</span>
  <span class="flex min-w-0 items-center gap-2">
    <code class="cursor-help truncate text-xs text-neutral-200" title={value}>{short}</code>
    <button type="button" class="mono shrink-0 border border-line px-2 py-0.5 text-[11px] text-neutral-400 hover:border-neutral-500 hover:text-neutral-100" onclick={copy} aria-label="Copy {label}">
      {copied ? 'copied' : 'copy'}
    </button>
  </span>
</div>
