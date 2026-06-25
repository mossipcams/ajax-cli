<script lang="ts">
  import type { BrowserCockpitView, BrowserTaskDetail } from "../types";
  import { statusMeta } from "../state";
  import { copyText } from "../diagnostics";
  import ActionBar from "./ActionBar.svelte";
  import PanePanel from "./PanePanel.svelte";

  interface Props {
    detail: BrowserTaskDetail;
    onBack?: () => void;
    onCockpit?: (cockpit: BrowserCockpitView) => void;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
    onMutated?: () => void;
  }

  let { detail, onBack, onCockpit, onResult, onMutated }: Props = $props();

  let meta = $derived(statusMeta(detail.status));
  let metaOpen = $state(false);
</script>

<div class="task-detail">
  <div class="detail-header">
    <button type="button" class="back" onclick={() => onBack?.()}>← Back</button>
    <h1 class="detail-title">{detail.title || detail.qualified_handle}</h1>
  </div>

  <section class="interact-panel">
    <div class="interact-state is-hero">
      <span class="interact-pill tone-{meta.tone}">{meta.label}</span>
      {#if detail.status_explanation}
        <span class="interact-summary">{detail.status_explanation}</span>
      {/if}
    </div>

    {#if detail.actions.length}
      <section class="next-action">
        <div class="interact-card-label">Next action</div>
        {#if detail.next_step}<p class="next-action-hint">{detail.next_step}</p>{/if}
        <ActionBar
          actions={detail.actions}
          handle={detail.qualified_handle}
          {onCockpit}
          {onResult}
          {onMutated}
        />
      </section>
    {/if}
  </section>

  <PanePanel handle={detail.qualified_handle} {detail} {onResult} />

  <details class="meta-details" bind:open={metaOpen}>
    <summary>Task details</summary>
    <div class="meta-group-label">Branch</div>
    <dl class="detail-grid">
      <dt>Branch</dt>
      <dd class="meta-copy-cell">
        <span class="meta-copy-value">{detail.branch}</span>
        <button type="button" class="meta-copy" onclick={() => copyText(detail.branch)}>Copy</button>
      </dd>
      <dt>Base</dt>
      <dd>{detail.base_branch}</dd>
      <dt>Worktree</dt>
      <dd class="meta-copy-cell">
        <span class="meta-copy-value">{detail.worktree_path}</span>
        <button type="button" class="meta-copy" onclick={() => copyText(detail.worktree_path)}>Copy</button>
      </dd>
      {#if detail.git?.unpushed_commits}
        <dt>Unpushed</dt>
        <dd>{detail.git.unpushed_commits}</dd>
      {/if}
    </dl>

    <div class="meta-group-label">Agent</div>
    <dl class="detail-grid">
      <dt>Client</dt>
      <dd>{detail.agent}</dd>
      <dt>Runtime</dt>
      <dd>{detail.agent_status}</dd>
      <dt>Tmux</dt>
      <dd>{detail.tmux_session}</dd>
    </dl>
  </details>
</div>
