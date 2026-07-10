<script lang="ts">
  import { RESULT_AUTO_DISMISS_MS, RESULT_SUCCESS_DISMISS_MS } from "../polling";

  interface Props {
    message: string;
    output?: string | null;
    isError?: boolean;
    onDismiss?: () => void;
  }

  let { message, output = null, isError = false, onDismiss }: Props = $props();

  let trimmedOutput = $derived(output?.trim() || null);

  // Success toasts dismiss in 4s so they overlay briefly and clear; errors keep
  // the longer window so output stays readable. Re-arm on message change.
  $effect(() => {
    void message;
    const dismissMs = isError ? RESULT_AUTO_DISMISS_MS : RESULT_SUCCESS_DISMISS_MS;
    const timer = setTimeout(() => onDismiss?.(), dismissMs);
    return () => clearTimeout(timer);
  });
</script>

<div class="result-panel" class:is-error={isError} role={isError ? "alert" : "status"} aria-live={isError ? "assertive" : "polite"}>
  <p class="result-message">{message}</p>
  {#if trimmedOutput}
    <pre class="result-output">{trimmedOutput}</pre>
  {/if}
  <button type="button" class="pill" onclick={() => onDismiss?.()}>Dismiss</button>
</div>
