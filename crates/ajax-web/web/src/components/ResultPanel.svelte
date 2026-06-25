<script lang="ts">
  import { RESULT_AUTO_DISMISS_MS } from "../polling";

  interface Props {
    message: string;
    output?: string | null;
    isError?: boolean;
    onDismiss?: () => void;
  }

  let { message, output = null, isError = false, onDismiss }: Props = $props();

  let trimmedOutput = $derived(output?.trim() || null);

  // Auto-dismiss mirrors the legacy 12s result timer. Re-arm whenever the
  // message changes so each result gets its own countdown.
  $effect(() => {
    void message;
    const timer = setTimeout(() => onDismiss?.(), RESULT_AUTO_DISMISS_MS);
    return () => clearTimeout(timer);
  });
</script>

<div class="result-panel" class:is-error={isError} role="status" aria-live="polite">
  <p class="result-message">{message}</p>
  {#if trimmedOutput}
    <pre class="result-output">{trimmedOutput}</pre>
  {/if}
  <button type="button" class="pill" onclick={() => onDismiss?.()}>Dismiss</button>
</div>
