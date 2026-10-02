<script lang="ts">
  type State = 'idle' | 'running' | 'valid' | 'rejected';
  let { state }: { state: State } = $props();
  const label = { idle: 'NOT VERIFIED', running: 'VERIFYING', valid: 'RECEIPT VALID', rejected: 'REJECTED' } as const;
  const tone = {
    idle: 'border-line text-neutral-400',
    running: 'border-neutral-600 text-neutral-300',
    valid: 'border-valid bg-valid/10 text-valid',
    rejected: 'border-bad bg-bad/10 text-bad'
  } as const;
</script>

<div role="status" aria-live="polite" class="mono inline-block border-2 px-6 py-3 text-2xl font-bold tracking-wider transition-colors sm:text-3xl {tone[state]}">
  {label[state]}
</div>
