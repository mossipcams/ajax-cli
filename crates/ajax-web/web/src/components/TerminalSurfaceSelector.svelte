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
  }

  function retryWterm() {
    initError = "";
    remountToken += 1;
  }
</script>

{#if v2Enabled}
  {#if initError}
    <p class="surface-fallback-error" data-testid="terminal-surface-v2-error">
      Terminal Surface V2 failed: {initError}
      <button type="button" class="surface-retry" onclick={retryWterm}>Retry</button>
    </p>
  {:else}
    {#key `${handle}:${remountToken}`}
      {#await import("./WtermTerminalView.svelte") then { default: WtermTerminalView }}
        <WtermTerminalView {handle} onInitFailure={handleInitFailure} />
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

<style>
  .surface-fallback-error {
    margin: 0 0 8px;
    padding: 8px 10px;
    font-size: 12px;
    line-height: 1.4;
    color: var(--ink);
    background: color-mix(in srgb, var(--mustard-bright) 18%, transparent);
    border: 1px solid var(--rule);
    border-radius: 8px;
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
    background: var(--surface-raised);
    color: var(--ink);
  }
</style>
