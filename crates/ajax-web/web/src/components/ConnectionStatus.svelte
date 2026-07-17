<script lang="ts">
  import type { ConnectionState } from "../types";

  interface Props {
    state: ConnectionState;
    detail?: string | null;
    healthHref?: string;
    onRetry?: () => void;
    onReload?: () => void;
    onCopyDiagnostics?: () => void;
  }

  let {
    state,
    detail = null,
    healthHref = "/api/health",
    onRetry,
    onReload,
    onCopyDiagnostics,
  }: Props = $props();

  let label = $derived(detail ? `${state}: ${detail}` : state);
</script>

<div class="connection-status" data-state={state}>
  <span class="connection-label">{label}</span>
  <div class="connection-actions" aria-label="Connection actions">
    <button type="button" class="is-primary" onclick={() => onRetry?.()}>Retry</button>
    <button type="button" onclick={() => onReload?.()}>Reload</button>
    <button type="button" onclick={() => onCopyDiagnostics?.()}>Copy Diagnostics</button>
    <a href={healthHref} target="_blank" rel="noreferrer">Open Health URL</a>
  </div>
</div>
