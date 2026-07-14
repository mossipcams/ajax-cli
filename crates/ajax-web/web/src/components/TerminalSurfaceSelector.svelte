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
  let fallbackToGhostty = $state(false);
  let fallbackMessage = $state("");

  $effect(() => {
    const unsubscribe = subscribeTerminalSurfaceV2((enabled) => {
      v2Enabled = enabled;
      if (enabled) {
        fallbackToGhostty = false;
        fallbackMessage = "";
      }
    });
    return unsubscribe;
  });

  const useWterm = $derived(v2Enabled && !fallbackToGhostty);

  function handleInitFailure(message: string) {
    fallbackToGhostty = true;
    fallbackMessage = message;
  }
</script>

{#key `${handle}:${useWterm}`}
  {#if useWterm}
    {#await import("./WtermTerminalView.svelte") then { default: WtermTerminalView }}
      <WtermTerminalView {handle} onInitFailure={handleInitFailure} />
    {/await}
  {:else}
    {#if fallbackMessage}
      <p class="surface-fallback-error" data-testid="terminal-surface-fallback-error">
        Terminal Surface V2 failed: {fallbackMessage}
      </p>
    {/if}
    {#await import("./TerminalRawView.svelte") then { default: TerminalRawView }}
      <TerminalRawView {handle} />
    {/await}
  {/if}
{/key}

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
  }
</style>
