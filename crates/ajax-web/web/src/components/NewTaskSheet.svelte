<script lang="ts">
  import { untrack } from "svelte";
  import type { BrowserCockpitView, RepoSummary } from "../types";
  import { requestId, startTask } from "../api";

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

<div
  id="new-task-sheet"
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
  <form class="sheet-card" autocomplete="off" onsubmit={submit}>
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
