<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    children?: Snippet;
    zIndex?: number;
  }

  let { children, zIndex = 50 }: Props = $props();
</script>

<div data-testid="fullscreen-layer" class="fullscreen-layer" style:z-index={zIndex}>
  {@render children?.()}
</div>

<style>
  .fullscreen-layer {
    position: fixed;
    /* Same visualViewport height pin as task/app keyboard bands. */
    top: var(--app-top, var(--app-band-top, 0px));
    left: 0;
    right: 0;
    height: var(--app-height, var(--app-band-height, 100dvh));
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-sizing: border-box;
  }
</style>
