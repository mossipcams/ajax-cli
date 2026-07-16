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
    /* Same live-band glue as task terminal / app-viewport keyboard pins. */
    top: var(--app-top, var(--app-band-top, 0px));
    left: 0;
    right: 0;
    bottom: max(
      0px,
      calc(100lvh - var(--app-top, 0px) - var(--app-height, 100lvh))
    );
    height: auto;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-sizing: border-box;
  }
</style>
