<script lang="ts">
  import { untrack } from "svelte";
  import type { BrowserCockpitView, RepoSummary } from "../types";
  import { requestId, startTask } from "../api";
  import { startTaskHandle } from "../taskSlug";
  import { sheetDrag } from "../gestures/sheetDragAction";
  import FullscreenLayer from "./FullscreenLayer.svelte";

  interface Props {
    repos: RepoSummary[];
    selectedProject?: string | null;
    onClose?: () => void;
    onCockpit?: (cockpit: BrowserCockpitView) => void;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
    onOpenTask?: (handle: string) => void;
  }

  let { repos, selectedProject = null, onClose, onCockpit, onResult, onOpenTask }: Props = $props();

  // Remembered form defaults — a browser-side convenience only, never task truth.
  const LAST_AGENT_KEY = "ajax.newTask.agent";
  const LAST_REPO_KEY = "ajax.newTask.repo";

  function readPref(key: string): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  }

  function savePrefs() {
    try {
      localStorage.setItem(LAST_AGENT_KEY, agent);
      localStorage.setItem(LAST_REPO_KEY, repo);
    } catch {
      // Private mode / storage denied: defaults just won't stick.
    }
  }

  const AGENTS = [
    { value: "codex", label: "Codex" },
    { value: "claude", label: "Claude" },
    { value: "cursor", label: "Cursor" },
    { value: "opencode", label: "OpenCode" },
  ];

  // Capture the initial repo once; the form then owns the selection locally.
  // Priority: the project the user is looking at, then the last-used repo.
  let repo = $state(
    untrack(() => {
      if (selectedProject && repos.some((r) => r.name === selectedProject)) return selectedProject;
      const remembered = readPref(LAST_REPO_KEY);
      if (remembered && repos.some((r) => r.name === remembered)) return remembered;
      return repos[0]?.name ?? "";
    }),
  );
  let title = $state("");
  let agent = $state(
    untrack(() => {
      const remembered = readPref(LAST_AGENT_KEY);
      return AGENTS.some((option) => option.value === remembered) ? remembered! : "codex";
    }),
  );
  let error = $state<string | null>(null);
  let submitting = $state(false);
  let dragOffset = $state(0);
  let sheetEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    sheetEl?.focus();
  });

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
      savePrefs();
      onResult?.("Task started", result.response.output, false);
      onOpenTask?.(startTaskHandle(repo, title));
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
  bind:this={sheetEl}
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
      enterkeyhint="go"
      placeholder="Short title"
      bind:value={title}
    />

    <span class="field-label" id="new-task-agent">Agent</span>
    <div class="agent-picker" role="radiogroup" aria-labelledby="new-task-agent">
      {#each AGENTS as option (option.value)}
        <button
          type="button"
          class="agent-option"
          class:is-selected={agent === option.value}
          role="radio"
          aria-checked={agent === option.value}
          onclick={() => (agent = option.value)}>{option.label}</button>
      {/each}
    </div>

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

  .sheet-card label,
  .sheet-card .field-label {
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

  /* Segmented agent picker — all choices visible, one tap, no dropdown scroll. */
  .agent-picker {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px;
    margin-top: 6px;
  }

  .agent-option {
    padding: 10px 12px;
    font-size: 14px;
    font-weight: 600;
    background: var(--paper);
    color: var(--ink-muted);
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
  }

  .agent-option.is-selected {
    background: var(--teal-deep);
    border-color: var(--teal);
    color: var(--paper);
  }

  .agent-option:focus-visible {
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
