<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    chrome: Snippet;
    children: Snippet;
    nav: Snippet;
  }

  let { chrome, children, nav }: Props = $props();
</script>

<div data-testid="app-shell" class="app-shell">
  {@render chrome()}
  <main data-testid="app-main" class="app-main">
    {@render children()}
  </main>
  {@render nav()}
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    height: 100%;
    overflow: hidden;
    width: 100%;
  }

  .app-main {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
    width: 100%;
  }
</style>
