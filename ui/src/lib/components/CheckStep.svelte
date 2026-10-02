<script lang="ts">
  type S = 'pass' | 'fail';
  let { label, status, shown, detail = '' }: { label: string; status: S; shown: boolean; detail?: string } = $props();
</script>

<li class="flex items-start gap-3 border border-line px-3 py-2 transition-opacity duration-300 {shown ? 'opacity-100' : 'opacity-0'}" aria-hidden={!shown}>
  <span class="mono mt-0.5 w-10 shrink-0 text-xs font-bold {status === 'pass' ? 'text-valid' : 'text-bad'}">{status === 'pass' ? 'PASS' : 'FAIL'}</span>
  <span class="min-w-0">
    <span class="block text-sm text-neutral-200">{label}</span>
    {#if detail}<span class="mono block break-all text-xs text-neutral-500">{detail}</span>{/if}
  </span>
</li>
