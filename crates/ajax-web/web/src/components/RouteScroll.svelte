<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();
</script>

<div data-testid="route-scroll" class="route-scroll">
  {@render children?.()}
</div>
