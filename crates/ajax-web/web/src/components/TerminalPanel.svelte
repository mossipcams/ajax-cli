<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { ZerolagInputAddon } from "xterm-zerolag-input";
  import { openTaskTerminalSocket } from "../api";
  import "@xterm/xterm/css/xterm.css";

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  let status = $state("connecting");

  onMount(() => {
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: "ui-monospace, SF Mono, Menlo, monospace",
      fontSize: 13,
      theme: {
        background: "#1c1714",
        foreground: "#f4eee0",
        cursor: "#52a095",
      },
    });
    const fitAddon = new FitAddon();
    const zerolag = new ZerolagInputAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(zerolag);

    if (container) {
      term.open(container);
      fitAddon.fit();
    }

    const socket = openTaskTerminalSocket(handle);

    const sendResize = () => {
      if (socket.readyState !== WebSocket.OPEN) return;
      socket.send(
        JSON.stringify({
          type: "resize",
          cols: term.cols,
          rows: term.rows,
        }),
      );
    };

    const resizeObserver =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(() => {
            fitAddon.fit();
            sendResize();
          })
        : null;
    if (container && resizeObserver) {
      resizeObserver.observe(container);
    }

    socket.addEventListener("open", () => {
      status = "connected";
      sendResize();
    });

    socket.addEventListener("message", (event) => {
      try {
        const payload = JSON.parse(String(event.data)) as { type?: string; data?: string };
        if (payload.type === "output" && payload.data) {
          const decoded = atob(payload.data);
          term.write(decoded);
          zerolag.clearFlushed();
          zerolag.rerender();
        } else if (payload.type === "error" && "error" in payload && payload.error) {
          status = String(payload.error);
        }
      } catch {
        term.write(String(event.data));
        zerolag.clearFlushed();
        zerolag.rerender();
      }
    });

    socket.addEventListener("error", () => {
      status = "error";
    });

    socket.addEventListener("close", () => {
      status = "closed";
    });

    term.onData((data) => {
      if (socket.readyState !== WebSocket.OPEN) return;

      if (data === "\r") {
        zerolag.clear();
        socket.send(JSON.stringify({ type: "input", data }));
        return;
      }

      if (data === "\x7f") {
        const source = zerolag.removeChar();
        if (source === "flushed") {
          socket.send(JSON.stringify({ type: "input", data }));
        }
        return;
      }

      if (data.length === 1 && data.charCodeAt(0) >= 32) {
        zerolag.addChar(data);
      }

      socket.send(JSON.stringify({ type: "input", data }));
    });

    return () => {
      resizeObserver?.disconnect();
      socket.close();
      zerolag.dispose();
      term.dispose();
    };
  });
</script>

<section class="terminal-panel" data-testid="task-terminal-panel" aria-label="Task terminal">
  <div class="terminal-host task-terminal-viewport" bind:this={container}></div>
  {#if status !== "connected"}
    <div class="terminal-status" data-testid="terminal-status">{status}</div>
  {/if}
</section>

<style>
  .terminal-panel {
    display: flex;
    flex-direction: column;
    min-height: 280px;
    height: min(58vh, 520px);
    margin-top: 16px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: var(--paper);
    overflow: hidden;
  }

  .terminal-host {
    flex: 1 1 auto;
    min-height: 0;
    padding: 8px;
  }

  .terminal-status {
    padding: 8px 12px;
    border-top: 1px solid var(--rule);
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  :global(.terminal-panel .xterm) {
    height: 100%;
  }

  :global(.terminal-panel .xterm-viewport) {
    overflow-y: auto;
  }
</style>
