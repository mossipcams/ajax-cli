<script lang="ts">
  import type { Snippet } from "svelte";
  import { initViewport } from "../viewport";

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();

  $effect(() => initViewport());
</script>

<div data-testid="app-viewport" class="app-viewport">
  {@render children?.()}
</div>

<style>
  /* Sole consumer of initViewport's --app-top / --app-height on <html>. */
  .app-viewport {
    --app-band-top: var(--app-top, 0px);
    --app-band-height: var(--app-height, 100dvh);
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    width: 100%;
    height: var(--app-band-height);
    max-height: var(--app-band-height);
    overflow: hidden;
    box-sizing: border-box;
  }

  :global(html.keyboard-open) .app-viewport {
    position: fixed;
    /* Height pin to visualViewport band. */
    top: var(--app-top, var(--app-band-top, 0px));
    left: 0;
    right: 0;
    height: var(--app-height, var(--app-band-height, 100dvh));
    max-height: var(--app-height, var(--app-band-height, 100dvh));
    z-index: 30;
  }
</style>
