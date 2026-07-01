<script lang="ts">
  import TerminalRawView from "./TerminalRawView.svelte";

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();
</script>

<section class="terminal-host-shell" data-testid="task-terminal">
  <TerminalRawView {handle} />
</section>

<style>
  .terminal-host-shell {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    margin-top: 16px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: var(--paper);
    overflow: hidden;
  }

  /* Landscape phones (coarse pointer, short viewport) stay on the mobile
     flex-fill layout even though they exceed the width breakpoint. */
  @media (min-width: 768px) and (not ((pointer: coarse) and (max-height: 500px))) {
    .terminal-host-shell {
      height: min(58vh, 560px);
    }
  }

  @media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {
    .terminal-host-shell {
      margin-top: 8px;
    }
  }
</style>
