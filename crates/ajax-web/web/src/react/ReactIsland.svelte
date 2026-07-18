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

<!-- display:contents so the mount wrapper never participates in layout: the
     mounted component's own root must be the direct flex/box child of the host
     slot (e.g. the task-detail terminal's height chain runs section →
     .task-detail, not through this div). -->
<div bind:this={host} style="display: contents"></div>
