<script lang="ts">
  import { onMount } from "svelte";
  import { ApiError, fetchDevDeploy, startDevDeploy } from "../api";
  import type { DevDeployStatus } from "../types";

  interface Props {
    taskHandle: string;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  }

  let { taskHandle, onResult }: Props = $props();

  let status = $state<DevDeployStatus | null>(null);
  let busy = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  const OPEN_URL = "https://ajaxdev.mossyhome.net:8788";

  async function refresh() {
    try {
      const response = await fetchDevDeploy();
      status = response.deploy;
      if (!response.deploy.active && pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
      }
    } catch {
      // Keep last known status; transient network blips during restart are expected.
    }
  }

  function startPolling() {
    if (pollTimer) return;
    pollTimer = setInterval(() => {
      void refresh();
    }, 1500);
  }

  async function deploy() {
    if (busy || status?.active) return;
    busy = true;
    try {
      const response = await startDevDeploy(taskHandle);
      status = response.deploy;
      startPolling();
      onResult?.("Test in Dev started", null, false);
    } catch (error) {
      const message =
        error instanceof ApiError ? error.message : "Test in Dev failed to start";
      onResult?.(message, null, true);
      await refresh();
    } finally {
      busy = false;
    }
  }

  function openDev() {
    window.open(OPEN_URL, "_blank", "noopener,noreferrer");
  }

  onMount(() => {
    void refresh();
    return () => {
      if (pollTimer) clearInterval(pollTimer);
    };
  });

  let phaseLabel = $derived(status?.phase_label ?? "Ready to deploy");
  let disabled = $derived(busy || !!status?.active);
  let error = $derived(status?.error ?? null);
</script>

<section class="test-in-dev" data-testid="test-in-dev" aria-label="Test in Dev">
  <div class="test-in-dev-row">
    <div class="actions">
      <button
        type="button"
        class="pill"
        data-testid="test-in-dev-button"
        disabled={disabled}
        onclick={() => void deploy()}
      >
        {disabled ? `${phaseLabel}…` : "Test in Dev"}
      </button>
      <button
        type="button"
        class="pill"
        data-testid="open-dev-button"
        onclick={openDev}
      >
        Open Dev
      </button>
    </div>
  </div>
  {#if error}
    <pre class="error" data-testid="test-in-dev-error">{error}</pre>
  {/if}
</section>

<style>
  .test-in-dev {
    margin: 0 0 12px;
  }

  .test-in-dev-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .pill {
    min-height: 24px;
    border: 1px solid var(--rule-strong);
    border-radius: 999px;
    background: transparent;
    color: var(--ink-muted);
    padding: 2px 8px;
    font-size: var(--text-micro);
    font-weight: 600;
    letter-spacing: var(--tracking-label);
    text-transform: uppercase;
  }

  .pill:hover,
  .pill:focus-visible {
    border-color: var(--ink-soft);
    color: var(--ink);
    outline: none;
  }

  .pill:disabled {
    opacity: 0.55;
  }

  .error {
    margin: 8px 0 0;
    padding: 8px 10px;
    border-radius: 8px;
    background: rgba(188, 92, 62, 0.12);
    color: var(--terracotta-bright);
    white-space: pre-wrap;
    font-size: 12px;
    max-height: 160px;
    overflow: auto;
  }
</style>
