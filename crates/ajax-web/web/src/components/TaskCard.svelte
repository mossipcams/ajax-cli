<script lang="ts">
  import type { BrowserCockpitView, BrowserTaskCard } from "../types";
  import { statusMeta, severityBucket } from "../state";
  import ActionBar from "./ActionBar.svelte";

  interface Props {
    card: BrowserTaskCard;
    severity?: number;
    onOpenTask?: (handle: string) => void;
    onCockpit?: (cockpit: BrowserCockpitView) => void;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
    onMutated?: () => void;
  }

  let { card, severity = 999, onOpenTask, onCockpit, onResult, onMutated }: Props = $props();

  let meta = $derived(statusMeta(card.status));
</script>

<article
  class="inbox-card tone-{meta.tone}"
  data-handle={card.qualified_handle}
  data-severity={severityBucket(severity)}
>
  <button
    type="button"
    class="inbox-card-open-body"
    onclick={() => onOpenTask?.(card.qualified_handle)}
  >
    <div class="inbox-card-head">
      <span class="status-dot tone-{meta.tone}" aria-hidden="true"></span>
      <span class="inbox-card-handle">{card.qualified_handle}</span>
      <span class="status-badge tone-{meta.tone}">{meta.label}</span>
    </div>
    {#if card.status_explanation}
      <p class="inbox-card-reason">{card.status_explanation}</p>
    {/if}
  </button>

  <div class="inbox-card-actions">
    <ActionBar
      actions={card.actions}
      handle={card.qualified_handle}
      {onCockpit}
      {onResult}
      {onMutated}
    />
    <button type="button" class="action" onclick={() => onOpenTask?.(card.qualified_handle)}>
      Open
    </button>
  </div>
</article>

<style>
  /* INBOX CARD — the part that earns the attention. The card is an <article>;
     its body is a button (.inbox-card-open-body) so actions stay tappable. */
  .inbox-card {
    position: relative;
    padding: var(--space-4) var(--space-4) var(--space-4) 17px;
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-left: 3px solid var(--tone, var(--rule-strong));
    border-radius: var(--radius-lg);
    box-shadow: var(--elev-1);
    transition: background 160ms var(--ease), transform 160ms var(--ease-spring),
      border-color 160ms var(--ease), box-shadow 160ms var(--ease);
  }

  .inbox-card:hover {
    background: var(--paper-high);
    box-shadow: var(--elev-2);
  }

  .inbox-card:active {
    transform: scale(0.995);
  }

  .inbox-card[data-severity="high"] {
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--tone-bg) 70%, transparent),
      var(--paper-raised) 42%
    );
  }

  .inbox-card-open-body {
    display: block;
    width: 100%;
    padding: 0;
    background: transparent;
    border: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .inbox-card-head {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .inbox-card-handle {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--ink);
  }

  .status-badge {
    flex: none;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    white-space: nowrap;
    color: var(--tone, var(--ink-muted));
    background: var(--tone-bg, transparent);
    padding: 3px 9px;
    border-radius: 999px;
  }

  .inbox-card-reason {
    margin: 8px 0 0;
    padding-left: 19px;
    font-size: 13px;
    line-height: 1.45;
    color: var(--ink-soft);
    overflow-wrap: anywhere;
  }

  .inbox-card-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 12px;
    padding-left: 19px;
  }

  @media (max-width: 380px) {
    .inbox-card-handle { font-size: 14px; }
  }
</style>
