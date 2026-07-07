<script lang="ts">
  import { untrack } from "svelte";
  import type { BrowserCockpitView, RepoSummary } from "../types";
  import { requestId, startTask } from "../api";
  import { sheetDrag } from "../gestures/sheetDragAction";
  import FullscreenLayer from "./FullscreenLayer.svelte";

  interface Props {
    repos: RepoSummary[];
    selectedProject?: string | null;
    onClose?: () => void;
    onCockpit?: (cockpit: BrowserCockpitView) => void;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  }

  let { repos, selectedProject = null, onClose, onCockpit, onResult }: Props = $props();

  // Capture the initial repo once; the form then owns the selection locally.
  let repo = $state(
    untrack(() =>
      selectedProject && repos.some((r) => r.name === selectedProject)
        ? selectedProject
        : (repos[0]?.name ?? ""),
    ),
  );
  let title = $state("");
  let agent = $state("codex");
  let error = $state<string | null>(null);
  let submitting = $state(false);
  let dragOffset = $state(0);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!repo) {
      error = "Pick a repository first";
      return;
    }
    if (!title.trim()) {
      error = "Add a title";
      return;
    }
    error = null;
    submitting = true;
    try {
      const result = await startTask({
        repo,
        title: title.trim(),
        agent,
        request_id: requestId(),
      });
      if (result.response.cockpit) onCockpit?.(result.response.cockpit);
      if (!result.ok) {
        error = result.error?.message ?? "Action failed";
        onResult?.(error, result.response.output, true);
        return;
      }
      onResult?.("Task started", result.response.output, false);
      onClose?.();
    } catch {
      error = "Action failed — network error";
      onResult?.("Could not start task", null, true);
    } finally {
      submitting = false;
    }
  }
</script>

<FullscreenLayer zIndex={50}>
<div
  id="new-task-sheet"
  data-testid="new-task-sheet"
  role="dialog"
  aria-modal="true"
  aria-labelledby="new-task-title"
  tabindex="-1"
  onclick={(event) => {
    if (event.target === event.currentTarget) onClose?.();
  }}
  onkeydown={(event) => {
    if (event.key === "Escape") onClose?.();
  }}
>
  <form
    class="sheet-card"
    autocomplete="off"
    onsubmit={submit}
    style="transform: translateY({dragOffset}px)"
    class:is-dragging={dragOffset > 0}
  >
    <div
      class="sheet-grab"
      aria-hidden="true"
      use:sheetDrag={{ onDismiss: () => onClose?.(), onOffset: (offset) => (dragOffset = offset) }}
    >
      <span class="sheet-grabber"></span>
    </div>
    <h2 id="new-task-title">New task</h2>

    <label for="new-task-repo">Repository</label>
    {#if repos.length}
      <select id="new-task-repo" bind:value={repo}>
        {#each repos as option (option.name)}
          <option value={option.name}>{option.name}</option>
        {/each}
      </select>
    {:else}
      <select id="new-task-repo" disabled>
        <option value="">No repositories configured</option>
      </select>
    {/if}

    <label for="new-task-title-input">Title</label>
    <input
      id="new-task-title-input"
      type="text"
      maxlength="80"
      placeholder="Short title"
      bind:value={title}
    />

    <label for="new-task-agent">Agent</label>
    <select id="new-task-agent" bind:value={agent}>
      <option value="codex">Codex</option>
      <option value="claude">Claude</option>
      <option value="cursor">Cursor</option>
      <option value="opencode">OpenCode</option>
    </select>

    {#if error}
      <p class="sheet-error">{error}</p>
    {/if}

    <div class="sheet-actions">
      <button type="button" class="pill" onclick={() => onClose?.()}>Cancel</button>
      <button type="submit" class="pill is-primary" disabled={submitting}>Start</button>
    </div>
  </form>
</div>
</FullscreenLayer>

<style>
  /* NEW TASK SHEET — bottom-rising modal inside FullscreenLayer's app band. */
  #new-task-sheet {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding: 20px;
    background: rgba(0, 0, 0, 0.6);
    overflow: hidden;
    box-sizing: border-box;
  }

  .sheet-card {
    background: var(--paper-raised);
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    padding: 22px;
    width: min(440px, 100%);
    max-height: calc(100% - 40px);
    margin-bottom: max(8px, env(safe-area-inset-bottom));
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
    animation: sheet-rise 220ms var(--ease-spring);
  }

  /* Spring the card back to rest when a drag is released below threshold. */
  .sheet-card:not(.is-dragging) {
    transition: transform 220ms var(--ease-spring);
  }

  /* Grabber — the touch target for drag-to-dismiss. */
  .sheet-grab {
    display: flex;
    justify-content: center;
    padding: 4px 0 12px;
    margin: -8px 0 0;
    cursor: grab;
    touch-action: none;
  }

  .sheet-grabber {
    width: 36px;
    height: 4px;
    border-radius: 999px;
    background: var(--rule-strong);
  }

  .sheet-card h2 {
    margin: 0 0 16px;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .sheet-card label {
    display: block;
    margin-top: 14px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .sheet-card input,
  .sheet-card select {
    width: 100%;
    margin-top: 5px;
    padding: 10px 12px;
    font-size: 16px;
    background: var(--paper);
    color: var(--ink);
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
  }

  .sheet-card input:focus,
  .sheet-card select:focus {
    outline: none;
    border-color: var(--teal-bright);
  }

  .sheet-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 22px;
  }

  .sheet-error {
    margin: 12px 0 0;
    color: var(--terracotta-bright);
    font-size: 12px;
  }

  @keyframes sheet-rise {
    from { opacity: 0; transform: translateY(16px); }
    to { opacity: 1; transform: translateY(0); }
  }

  @media (max-width: 380px) {
    .sheet-card { padding: 18px; }
  }
</style>
