<script lang="ts">
  import { onMount } from "svelte";
  import type { ComponentType } from "react";
  import { mountIsland, type IslandHandle } from "./mountIsland";

  interface Props {
    component: ComponentType<any>;
    props: Record<string, unknown>;
  }

  let { component, props }: Props = $props();

  let host: HTMLDivElement;
  let island: IslandHandle<Record<string, unknown>> | undefined;

  onMount(() => {
    island = mountIsland(host, component, props);
    return () => {
      island?.unmount();
    };
  });

  $effect(() => {
    island?.update(props);
  });
</script>

<div bind:this={host}></div>
