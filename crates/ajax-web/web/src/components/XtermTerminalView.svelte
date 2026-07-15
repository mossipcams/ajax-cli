<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import {
    connectTaskTerminal,
    type TerminalConnection,
    type TerminalConnectionStatus,
  } from "../terminalConnection";
  import { MIN_TERMINAL_COLS } from "../terminalGeometry";

  interface Props {
    handle: string;
    onInitFailure?: (message: string) => void;
  }

  let { handle, onInitFailure }: Props = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let term: Terminal | undefined = $state();
  let connection: TerminalConnection | undefined = $state();
  let status = $state<TerminalConnectionStatus>("connecting");
  let statusDetail = $state("");
  let ctrlArmed = $state(false);

  const CONTROL_KEYS = [
    { label: "Esc", data: "\x1b" },
    { label: "Tab", data: "\t" },
    { label: "⌃C", data: "\x03" },
    { label: "←", data: "\x1b[D" },
    { label: "↑", data: "\x1b[A" },
    { label: "↓", data: "\x1b[B" },
    { label: "→", data: "\x1b[C" },
  ];

  const CTRL_ARM_TIMEOUT_MS = 4000;
  let ctrlTimer: ReturnType<typeof setTimeout> | undefined;

  const STATUS_LABELS: Record<TerminalConnectionStatus, string> = {
    connecting: "Connecting…",
    connected: "Connected",
    reconnecting: "Reconnecting…",
    unavailable: "Unavailable",
  };

  const disarmCtrl = () => {
    ctrlArmed = false;
    if (ctrlTimer) {
      clearTimeout(ctrlTimer);
      ctrlTimer = undefined;
    }
  };

  const toggleCtrl = () => {
    if (ctrlArmed) {
      disarmCtrl();
      return;
    }
    ctrlArmed = true;
    if (ctrlTimer) clearTimeout(ctrlTimer);
    ctrlTimer = setTimeout(disarmCtrl, CTRL_ARM_TIMEOUT_MS);
  };

  const controlModify = (data: string): string => {
    if (data.length === 1) {
      const code = data.toLowerCase().charCodeAt(0);
      if (code >= 97 && code <= 122) return String.fromCharCode(code - 96);
    }
    const cursor = /^\x1b\[([ABCD])$/.exec(data);
    if (cursor) return `\x1b[1;5${cursor[1]}`;
    return data;
  };

  const consumeCtrl = (data: string): string => {
    if (!ctrlArmed) return data;
    disarmCtrl();
    return controlModify(data);
  };

  const sendKey = (data: string) => {
    if (!connection?.isOpen()) return;
    connection.sendInput(data);
  };

  const refocusTerm = () => {
    term?.focus();
  };

  const requestPaste = async () => {
    try {
      const text = await navigator.clipboard?.readText?.();
      if (text) sendKey(text);
    } catch {
      // Clipboard denied — use native long-press paste in the terminal host.
    }
  };

  const requestReconnect = () => {
    connection?.reconnectNow();
  };

  onMount(() => {
    let fitAddon: FitAddon | undefined;
    let resizeObserver: ResizeObserver | undefined;

    const reportResize = () => {
      fitAddon?.fit();
      if (!term) return;
      connection?.sendResize(Math.max(term.cols, MIN_TERMINAL_COLS), term.rows);
    };

    try {
      if (!hostEl) {
        throw new Error("terminal host missing");
      }
      const liveTerm = new Terminal({
        fontSize: 13,
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
        cursorBlink: true,
        scrollback: 2000,
      });
      fitAddon = new FitAddon();
      liveTerm.loadAddon(fitAddon);
      liveTerm.open(hostEl);
      fitAddon.fit();
      term = liveTerm;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      onInitFailure?.(message);
      return () => {
        if (ctrlTimer) clearTimeout(ctrlTimer);
      };
    }

    term.onData((data) => sendKey(consumeCtrl(data)));

    connection = connectTaskTerminal(handle, {
      onOutput: (text) => term?.write(text),
      onServerError: (message) => {
        statusDetail = message;
      },
      onStatus: (next) => {
        status = next;
      },
      onOpen: () => {
        statusDetail = "";
        reportResize();
        requestAnimationFrame(() => term?.focus());
      },
    });

    reportResize();

    const onWindowResize = () => reportResize();
    resizeObserver = new ResizeObserver(() => reportResize());
    resizeObserver.observe(hostEl);
    window.addEventListener("resize", onWindowResize);

    return () => {
      resizeObserver?.disconnect();
      window.removeEventListener("resize", onWindowResize);
      if (ctrlTimer) clearTimeout(ctrlTimer);
      connection?.dispose();
      fitAddon?.dispose();
      term?.dispose();
      connection = undefined;
      term = undefined;
    };
  });
</script>

<div class="xterm-root">
  <section
    class="terminal-panel"
    data-testid="task-terminal-panel"
    data-terminal-engine="xterm"
    aria-label="Task terminal">
    <div class="terminal-host xterm-host" bind:this={hostEl}></div>
    <div
      class="terminal-status"
      class:is-empty={!(status !== "connected" || statusDetail)}
      data-testid="terminal-status"
      aria-hidden={!(status !== "connected" || statusDetail)}>
      {#if status !== "connected" || statusDetail}
        <span class="terminal-status-label">{STATUS_LABELS[status]}</span>
        {#if statusDetail}
          <span class="terminal-status-detail">{statusDetail}</span>
        {/if}
        {#if status === "reconnecting" || status === "unavailable"}
          <button
            type="button"
            class="terminal-status-reconnect"
            onclick={() => requestReconnect()}>Reconnect</button>
        {/if}
      {/if}
    </div>
    <div class="terminal-keys" role="toolbar" aria-label="Terminal keys">
      {#each CONTROL_KEYS as key (key.label)}
        <button
          type="button"
          class="terminal-key"
          onmousedown={(event) => event.preventDefault()}
          onclick={() => {
            sendKey(consumeCtrl(key.data));
            refocusTerm();
          }}>{key.label}</button>
      {/each}
      <button
        type="button"
        class="terminal-key"
        class:is-armed={ctrlArmed}
        aria-pressed={ctrlArmed}
        onmousedown={(event) => event.preventDefault()}
        onclick={() => {
          toggleCtrl();
          refocusTerm();
        }}>Ctrl{#if ctrlArmed}<span class="terminal-key-armed-dot" aria-hidden="true"></span>{/if}</button>
      <button
        type="button"
        class="terminal-key"
        onmousedown={(event) => event.preventDefault()}
        onclick={() => void requestPaste()}>Paste</button>
      <button
        type="button"
        class="terminal-key"
        aria-label="Hide keyboard"
        onclick={() => {
          (document.activeElement as HTMLElement | null)?.blur();
        }}>⌄</button>
    </div>
  </section>
</div>

<style>
  .xterm-root {
    display: contents;
  }

  .terminal-panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 auto;
  }

  .xterm-host {
    flex: 1 1 auto;
    min-height: 120px;
    overflow: hidden;
  }

  .terminal-keys {
    display: flex;
    flex-wrap: nowrap;
    gap: 4px;
    padding: 2px 4px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .terminal-keys::-webkit-scrollbar {
    display: none;
  }

  .terminal-key {
    flex: none;
    min-height: 28px;
    padding: 1px 7px;
    font-size: 11px;
    border-radius: 6px;
    border: 1px solid var(--rule);
    background: var(--surface-raised);
    color: var(--ink);
  }

  .terminal-key.is-armed {
    border-color: var(--mustard-bright);
  }

  .terminal-key-armed-dot {
    display: inline-block;
    width: 4px;
    height: 4px;
    margin-left: 2px;
    border-radius: 50%;
    background: var(--mustard-bright);
    vertical-align: middle;
  }

  .terminal-status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    font-size: 12px;
    color: var(--ink-muted);
  }

  .terminal-status.is-empty {
    display: none;
  }

  .terminal-status-detail {
    font-family: var(--mono);
    overflow-wrap: anywhere;
  }

  .terminal-status-reconnect {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--rule);
    background: var(--surface-raised);
    color: var(--ink);
  }
</style>
