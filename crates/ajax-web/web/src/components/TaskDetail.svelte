<script lang="ts">
  import type { BrowserCockpitView, BrowserTaskDetail } from "../types";
  import { statusMeta } from "../state";
  import { copyText } from "../diagnostics";
  import { visibleTaskActions } from "../taskActions";
  import ActionBar from "./ActionBar.svelte";
  import TerminalRawView from "./TerminalRawView.svelte";

  interface Props {
    detail: BrowserTaskDetail;
    onBack?: () => void;
    onCockpit?: (cockpit: BrowserCockpitView) => void;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
    onMutated?: () => void;
  }

  let { detail, onBack, onCockpit, onResult, onMutated }: Props = $props();

  let meta = $derived(statusMeta(detail.status));
  let actions = $derived(visibleTaskActions(detail.actions));
  let metaOpen = $state(false);
</script>

<div class="task-detail is-terminal-first">
  <div class="detail-header" data-mobile-chrome="header">
    <button type="button" class="back" onclick={() => onBack?.()}>← Back</button>
    <h1 class="detail-title">{detail.title || detail.qualified_handle}</h1>
    <span class="interact-pill tone-{meta.tone}">{meta.label}</span>
  </div>

  <section class="interact-panel" data-mobile-chrome="actions">
    {#if detail.status_explanation}
      <p class="interact-summary">{detail.status_explanation}</p>
    {/if}
    {#if actions.length}
      <ActionBar
        {actions}
        handle={detail.qualified_handle}
        {onCockpit}
        {onResult}
        {onMutated}
      />
    {/if}
  </section>

  <div class="terminal-primary" data-mobile-primary="terminal">
    <TerminalRawView handle={detail.qualified_handle} />
  </div>

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

<style>
  /* DETAIL HEADER --------------------------------------------------------- */
  .detail-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 18px;
  }

  .detail-header .back {
    flex: none;
    display: inline-flex;
    align-items: center;
    min-height: 44px;
    background: transparent;
    border: 1px solid var(--rule-strong);
    border-radius: 999px;
    padding: 7px 16px;
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }

  .detail-header .back:hover,
  .detail-header .back:focus-visible {
    border-color: var(--ink-soft);
    color: var(--ink);
    outline: none;
  }

  .detail-title {
    margin: 0;
    font-size: 21px;
    font-weight: 700;
    letter-spacing: 0.01em;
    line-height: 1.25;
    text-transform: none;
    color: var(--ink);
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .detail-header .interact-pill {
    flex: none;
    margin-left: auto;
  }

  /* STATUS PILL — lives in the header row so state is always in view ------- */
  .interact-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: 999px;
    border: 1px solid var(--rule-strong);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink);
  }

  .interact-pill.tone-running {
    background: var(--teal-deep);
    border-color: var(--teal);
  }

  .interact-pill.tone-running::before {
    content: "";
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--teal-bright);
    animation: pulse 2.2s ease-in-out infinite;
  }

  .interact-pill.tone-waiting,
  .interact-pill.tone-attention {
    background: rgba(201, 162, 74, 0.18);
    border-color: var(--mustard);
    color: var(--mustard-bright);
  }

  .interact-pill.tone-error,
  .interact-pill.tone-danger {
    background: rgba(188, 92, 62, 0.18);
    border-color: var(--terracotta);
    color: var(--terracotta-bright);
  }

  .interact-pill.tone-success {
    background: rgba(54, 112, 105, 0.28);
    border-color: var(--teal);
  }

  .interact-pill.tone-idle,
  .interact-pill.tone-muted {
    background: transparent;
    border-color: var(--rule-strong);
    color: var(--ink-muted);
  }

  .interact-summary {
    margin: 0 0 12px;
    font-size: 14px;
    line-height: 1.45;
    color: var(--ink-soft);
    overflow-wrap: anywhere;
  }

  .interact-summary:last-child {
    margin-bottom: 0;
  }

  /* META DETAILS ---------------------------------------------------------- */
  .meta-details {
    margin-top: 18px;
    border-top: 1px solid var(--rule);
    padding-top: 16px;
  }

  .meta-details summary {
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .meta-group-label {
    margin: 14px 0 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-faint);
  }

  .detail-grid {
    display: grid;
    grid-template-columns: 104px 1fr;
    gap: 8px 14px;
    font-size: 13px;
  }

  .detail-grid dt {
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    font-weight: 600;
  }

  .detail-grid dd {
    margin: 0;
    color: var(--ink);
    overflow-wrap: anywhere;
    font-feature-settings: "tnum";
  }

  .meta-copy-cell {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .meta-copy-value {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .meta-copy {
    flex: none;
    min-height: 28px;
    padding: 4px 10px;
    background: transparent;
    border: 1px solid var(--rule-strong);
    border-radius: 999px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .meta-copy:hover,
  .meta-copy:focus-visible {
    border-color: var(--ink-soft);
    color: var(--ink);
    outline: none;
  }

  /* Mobile: tighten chrome for terminal-first layout inside route-scroll.
     Includes landscape phones (coarse pointer, short viewport). */
  @media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {
    :global(html.terminal-expanded),
    :global(html.terminal-expanded body),
    :global(html.keyboard-open),
    :global(html.keyboard-open body) {
      overflow: hidden;
      overscroll-behavior: none;
    }

    /* Task detail scrolls with route-scroll; flex column fills the app band. */
    .task-detail {
      display: flex;
      flex-direction: column;
      min-height: var(--app-band-height, 100dvh);
      padding: env(safe-area-inset-top) 0 0;
      background: var(--paper);
      overflow: visible;
    }

    .detail-header,
    .interact-panel {
      padding-left: calc(12px + env(safe-area-inset-left));
      padding-right: calc(12px + env(safe-area-inset-right));
    }

    .detail-header { margin-bottom: 4px; }
    .detail-header .back { min-height: 32px; padding: 4px 12px; }
    .detail-title { font-size: 18px; line-height: 1.15; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .interact-summary { display: none; }
    /* Meta details stay on desktop; on mobile they sit below the terminal in
       route-scroll and are hidden so the terminal gets more height. */
    .meta-details { display: none; }
    .terminal-primary {
      display: flex;
      flex: 1 1 auto;
      min-height: 0;
    }
  }

  @media (max-width: 380px) {
    .detail-title { font-size: 19px; }
    .detail-grid { grid-template-columns: 92px 1fr; }
  }
</style>
