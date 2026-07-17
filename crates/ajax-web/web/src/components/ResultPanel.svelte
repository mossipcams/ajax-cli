<script lang="ts">
  import { DROP_UNDO_MS, RESULT_AUTO_DISMISS_MS, RESULT_SUCCESS_DISMISS_MS } from "../polling";

  interface Props {
    message: string;
    output?: string | null;
    isError?: boolean;
    onDismiss?: () => void;
    /** Cancel a pending pre-commit action (e.g. delayed Drop). */
    onUndo?: () => void;
    /** Commit a pending pre-commit action when the undo window elapses. */
    onCommit?: () => void;
  }

  let { message, output = null, isError = false, onDismiss, onUndo, onCommit }: Props = $props();

  let trimmedOutput = $derived(output?.trim() || null);
  // When armed with undo/commit, the toast stays open for the undo window and
  // the timer commits; otherwise success dismisses fast and errors linger.
  let undoArmed = $derived(!!onUndo || !!onCommit);

  $effect(() => {
    void message;
    const dismissMs = undoArmed
      ? DROP_UNDO_MS
      : isError
        ? RESULT_AUTO_DISMISS_MS
        : RESULT_SUCCESS_DISMISS_MS;
    const timer = setTimeout(() => {
      if (undoArmed) onCommit?.();
      onDismiss?.();
    }, dismissMs);
    return () => clearTimeout(timer);
  });

  // Dismiss during the pending window is a safe cancel (same as Undo).
  function dismiss() {
    if (undoArmed) onUndo?.();
    onDismiss?.();
  }
</script>

<div class="result-panel" class:is-error={isError} role={isError ? "alert" : "status"} aria-live={isError ? "assertive" : "polite"}>
  <p class="result-message">{message}</p>
  {#if trimmedOutput}
    <pre class="result-output">{trimmedOutput}</pre>
  {/if}
  {#if undoArmed}
    <button type="button" class="pill is-primary" onclick={dismiss}>Undo</button>
  {/if}
  <button type="button" class="pill" onclick={dismiss}>Dismiss</button>
</div>
