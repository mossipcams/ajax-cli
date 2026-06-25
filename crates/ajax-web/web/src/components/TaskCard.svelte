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
