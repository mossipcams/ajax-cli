<script lang="ts">
  import TerminalRawView from "./TerminalRawView.svelte";
  import TerminalSnapshotView from "./TerminalSnapshotView.svelte";

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  type Mode = "live" | "raw";
  const STORAGE_KEY = "ajax.terminal.mode";

  const isMobileViewport = (): boolean => {
    if (typeof window.matchMedia === "function") {
      return window.matchMedia("(max-width: 767px)").matches;
    }
    return (navigator.maxTouchPoints ?? 0) > 0;
  };

  // Mobile lands on the snapshot viewer + composer: it survives backgrounding,
  // handles alt-screen agents, and needs no raw attach socket. The raw
  // tmux-attached terminal is a deliberate opt-in (and the desktop default),
  // kept for full-fidelity interactive/TUI moments and debugging. An explicit
  // choice persists so returning users keep what they picked.
  const initialMode = (): Mode => {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved === "live" || saved === "raw") return saved;
    } catch {
      // localStorage can throw in private mode; fall back to the viewport default.
    }
    return isMobileViewport() ? "live" : "raw";
  };

  let mode = $state<Mode>(initialMode());

  const setMode = (next: Mode) => {
    mode = next;
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Non-fatal: the mode still applies for this session.
    }
  };
</script>

<section class="terminal-host-shell" data-testid="task-terminal">
  <div class="terminal-mode-toggle" role="tablist" aria-label="Terminal mode">
    <button
      type="button"
      role="tab"
      class="terminal-mode-tab"
      class:is-active={mode === "live"}
      aria-selected={mode === "live"}
      onclick={() => setMode("live")}>Live</button>
    <button
      type="button"
      role="tab"
      class="terminal-mode-tab"
      class:is-active={mode === "raw"}
      aria-selected={mode === "raw"}
      onclick={() => setMode("raw")}>Raw terminal</button>
  </div>

  {#if mode === "raw"}
    <TerminalRawView {handle} />
  {:else}
    <TerminalSnapshotView {handle} />
  {/if}
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

  @media (min-width: 768px) {
    .terminal-host-shell {
      height: min(58vh, 560px);
    }
  }

  @media (max-width: 767px) {
    .terminal-host-shell {
      margin-top: 8px;
    }
  }

  .terminal-mode-toggle {
    display: flex;
    gap: 6px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--rule);
  }

  .terminal-mode-tab {
    flex: none;
    min-height: 36px;
    padding: 6px 14px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .terminal-mode-tab.is-active {
    background: var(--teal-deep);
    border-color: var(--teal);
    color: var(--paper);
  }
</style>
