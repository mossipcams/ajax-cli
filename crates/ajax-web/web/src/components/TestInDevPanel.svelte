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
  let occupant = $derived(status?.occupant ?? null);
  let error = $derived(status?.error ?? null);
</script>

<section class="test-in-dev" data-testid="test-in-dev" aria-label="Test in Dev">
  <div class="test-in-dev-head">
    <h2>Test in Dev</h2>
    <span class="phase" data-testid="test-in-dev-phase">{phaseLabel}</span>
  </div>
  <p class="note">
    Shared Ajax Dev slot only — deploys this worktree (including uncommitted changes) to
    the existing Dev instance. Does not create a preview URL or touch GitHub.
  </p>
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
  {#if occupant}
    <dl class="occupant" data-testid="test-in-dev-occupant">
      <div><dt>Task</dt><dd>{occupant.title || occupant.task_handle}</dd></div>
      <div><dt>Branch</dt><dd>{occupant.branch}</dd></div>
      <div><dt>Commit</dt><dd>{occupant.commit_sha}{occupant.dirty ? " (dirty)" : ""}</dd></div>
      {#if occupant.deployed_at_unix_secs}
        <div>
          <dt>Deployed</dt>
          <dd>{new Date(occupant.deployed_at_unix_secs * 1000).toLocaleString()}</dd>
        </div>
      {/if}
    </dl>
  {/if}
  {#if error}
    <pre class="error" data-testid="test-in-dev-error">{error}</pre>
  {/if}
</section>

<style>
  .test-in-dev {
    margin: 0 0 16px;
    padding: 14px 16px;
    border: 1px solid var(--rule-strong);
    border-radius: 14px;
    background: rgba(54, 112, 105, 0.08);
  }

  .test-in-dev-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }

  .test-in-dev-head h2 {
    margin: 0;
    font-size: 13px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink);
  }

  .phase {
    margin-left: auto;
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }

  .note {
    margin: 0 0 12px;
    font-size: 13px;
    line-height: 1.4;
    color: var(--ink-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .pill {
    min-height: 44px;
    border: 1px solid var(--rule-strong);
    border-radius: 999px;
    background: transparent;
    color: var(--ink);
    padding: 8px 16px;
    font-size: 12px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .pill:disabled {
    opacity: 0.55;
  }

  .occupant {
    margin: 12px 0 0;
    display: grid;
    gap: 6px;
  }

  .occupant div {
    display: grid;
    grid-template-columns: 88px 1fr;
    gap: 8px;
    font-size: 13px;
  }

  .occupant dt {
    color: var(--ink-muted);
  }

  .occupant dd {
    margin: 0;
    color: var(--ink);
    overflow-wrap: anywhere;
  }

  .error {
    margin: 12px 0 0;
    padding: 10px 12px;
    border-radius: 10px;
    background: rgba(188, 92, 62, 0.12);
    color: var(--terracotta-bright);
    white-space: pre-wrap;
    font-size: 12px;
    max-height: 160px;
    overflow: auto;
  }
</style>
