<script lang="ts">
  import {
    isTerminalSurfaceV2Enabled,
    subscribeTerminalSurfaceV2,
  } from "../terminalSurfaceSetting";

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  let v2Enabled = $state(isTerminalSurfaceV2Enabled());
  let initError = $state("");
  let remountToken = $state(0);

  $effect(() => {
    const unsubscribe = subscribeTerminalSurfaceV2((enabled) => {
      if (v2Enabled === enabled) return;
      v2Enabled = enabled;
      initError = "";
      remountToken += 1;
    });
    return unsubscribe;
  });

  function handleInitFailure(message: string) {
    initError = message;
    try {
      sessionStorage.setItem("ajax.terminal.surfaceV2.lastError", message);
    } catch {
      // Best-effort for Dev settings debug panel.
    }
  }

  function retrySurfaceV2() {
    initError = "";
    remountToken += 1;
    try {
      sessionStorage.removeItem("ajax.terminal.surfaceV2.lastError");
    } catch {
      // Best-effort.
    }
  }
</script>

<div class="surface-selector">
{#if v2Enabled}
  {#if initError}
    <p class="surface-fallback-error" data-testid="terminal-surface-v2-error">
      Terminal Surface V2 unavailable: {initError}
      <button type="button" class="surface-retry" onclick={retrySurfaceV2}>Retry</button>
    </p>
  {:else}
    {#key `${handle}:${remountToken}`}
      {#await import("./XtermTerminalView.svelte") then { default: XtermTerminalView }}
        <XtermTerminalView {handle} onInitFailure={handleInitFailure} />
      {/await}
    {/key}
  {/if}
{:else}
  {#key handle}
    {#await import("./TerminalRawView.svelte") then { default: TerminalRawView }}
      <TerminalRawView {handle} />
    {/await}
  {/key}
{/if}
</div>

<style>
  /* Layout-transparent: Ghostty roots must flex directly under .terminal-primary.
     A real flex wrapper here shrinks the Ghostty host so the bottom input
     textarea covers swipe targets and e2e dragTo hangs. */
  .surface-selector {
    display: contents;
  }

  .surface-fallback-error {
    margin: 0 0 8px;
    padding: 6px 8px;
    font-size: 12px;
    line-height: 1.4;
    color: var(--ink);
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-left: 3px solid var(--mustard-bright);
    border-radius: 6px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .surface-retry {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--rule);
    background: var(--paper-raised);
    color: var(--ink);
  }
</style>
