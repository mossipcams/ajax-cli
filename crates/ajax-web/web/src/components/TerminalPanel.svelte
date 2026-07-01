<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { ZerolagInputAddon } from "xterm-zerolag-input";
  import { openTaskTerminalSocket } from "../api";
  import { wheelNotchesFromDrag } from "../terminalTouchScroll";
  import "@xterm/xterm/css/xterm.css";

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  let status = $state("connecting");
  let ctrlArmed = $state(false);

  // Assigned inside onMount so the key bar can reach the live socket/terminal.
  let sendKey: (data: string) => void = () => {};
  let focusTerm: () => void = () => {};

  // Control-key bar: keys the iOS keyboard lacks. The "Ctrl" button is a sticky
  // modifier folded into the next typed letter by the onData handler below.
  const CONTROL_KEYS = [
    { label: "Esc", data: "\x1b" },
    { label: "Tab", data: "\t" },
    { label: "⌃C", data: "\x03" },
    { label: "←", data: "\x1b[D" },
    { label: "↑", data: "\x1b[A" },
    { label: "↓", data: "\x1b[B" },
    { label: "→", data: "\x1b[C" },
  ];

  onMount(() => {
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: "ui-monospace, SF Mono, Menlo, monospace",
      fontSize: 14,
      theme: {
        background: "#1c1714",
        foreground: "#f4eee0",
        cursor: "#52a095",
      },
    });
    const fitAddon = new FitAddon();
    const zerolag = new ZerolagInputAddon();
    const outputDecoder = new TextDecoder();
    term.loadAddon(fitAddon);
    term.loadAddon(zerolag);

    if (container) {
      term.open(container);
      // Harden the hidden textarea xterm owns so mobile keyboards don't corrupt
      // terminal input with autocorrect/autocapitalize.
      const input = term.textarea;
      if (input) {
        input.setAttribute("autocapitalize", "off");
        input.setAttribute("autocorrect", "off");
        input.setAttribute("autocomplete", "off");
        input.setAttribute("spellcheck", "false");
      }
      // Defer the first fit until layout settles so the PTY gets real dimensions.
      requestAnimationFrame(() => fitAddon.fit());
    }

    // Auto-follow new output only while the user is at the bottom of the
    // scrollback. A tmux-attached session redraws constantly (status bar,
    // idle prompt refresh), and unconditionally calling scrollToBottom() on
    // every output frame yanked the view back down the instant a user tried
    // to scroll up — scrolling looked completely broken.
    let pinnedToBottom = true;
    term.onScroll(() => {
      pinnedToBottom = term.buffer.active.viewportY >= term.buffer.active.baseY;
    });

    // Touch-drag scrolling. xterm 6 ships VS Code's touch-gesture code but
    // never wires it up, and its `.xterm-screen` overlays the scrollable
    // `.xterm-viewport`, so native touch scrolling never fires — the terminal
    // was completely unscrollable on iOS. We translate vertical drags into
    // synthetic wheel events dispatched at the xterm root (`term.element`),
    // where xterm binds both its wheel handlers. This reuses xterm's own
    // wheel logic: it forwards scroll to tmux when the pane is in mouse mode
    // (our attach) or scrolls xterm's local scrollback otherwise. A stationary
    // tap still focuses the textarea to type; only a drag scrolls, so scrolling
    // no longer pops the iOS keyboard.
    const TOUCH_SCROLL_THRESHOLD_PX = 6;
    let touchActive = false;
    let touchLastY = 0;
    let touchAccumPx = 0;

    const cellHeightPx = (): number => {
      const viewport = container?.querySelector<HTMLElement>(".xterm-viewport");
      const height = viewport?.clientHeight ?? 0;
      // jsdom and pre-layout paints report 0; fall back to a sane line height.
      return height > 0 && term.rows > 0 ? height / term.rows : 18;
    };

    const dispatchWheel = (deltaY: number, clientX: number, clientY: number) => {
      term.element?.dispatchEvent(
        new WheelEvent("wheel", {
          deltaY,
          deltaMode: WheelEvent.DOM_DELTA_LINE,
          clientX,
          clientY,
          bubbles: true,
          cancelable: true,
        }),
      );
    };

    const onTouchStart = (event: TouchEvent) => {
      if (event.touches.length !== 1) {
        touchActive = false;
        return;
      }
      touchActive = true;
      touchAccumPx = 0;
      touchLastY = event.touches[0].clientY;
    };

    const onTouchMove = (event: TouchEvent) => {
      if (!touchActive || event.touches.length !== 1) return;
      const touch = event.touches[0];
      touchAccumPx += touchLastY - touch.clientY;
      touchLastY = touch.clientY;
      if (Math.abs(touchAccumPx) < TOUCH_SCROLL_THRESHOLD_PX) return;

      const { notches, remainderPx } = wheelNotchesFromDrag(touchAccumPx, cellHeightPx());
      touchAccumPx = remainderPx;
      if (notches === 0) return;

      // A moved touch is a scroll, not a tap: stop the page from rubber-banding
      // and stop iOS from synthesizing the click that would open the keyboard.
      if (event.cancelable) event.preventDefault();
      const step = notches > 0 ? 1 : -1;
      for (let i = 0; i < Math.abs(notches); i += 1) {
        dispatchWheel(step, touch.clientX, touch.clientY);
      }
    };

    const onTouchEnd = () => {
      touchActive = false;
      touchAccumPx = 0;
    };

    container?.addEventListener("touchstart", onTouchStart, { passive: true });
    container?.addEventListener("touchmove", onTouchMove, { passive: false });
    container?.addEventListener("touchend", onTouchEnd, { passive: true });
    container?.addEventListener("touchcancel", onTouchEnd, { passive: true });

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

    sendKey = (data: string) => {
      if (socket.readyState !== WebSocket.OPEN) return;
      socket.send(JSON.stringify({ type: "input", data }));
    };
    focusTerm = () => term.focus();

    // Coalesce refits triggered by container, window, orientation, and the
    // on-screen keyboard (visualViewport) into one rAF.
    let refitFrame = 0;
    const scheduleRefit = () => {
      if (refitFrame) cancelAnimationFrame(refitFrame);
      refitFrame = requestAnimationFrame(() => {
        fitAddon.fit();
        sendResize();
        // A viewport shrink (keyboard open) refits to fewer rows; keep the
        // cursor/newest output visible rather than stranded above the fold.
        term.scrollToBottom();
      });
    };

    const schedulePostLayoutRefit = () => {
      scheduleRefit();
      requestAnimationFrame(scheduleRefit);
    };

    const resizeObserver =
      typeof ResizeObserver !== "undefined" ? new ResizeObserver(scheduleRefit) : null;
    if (container && resizeObserver) {
      resizeObserver.observe(container);
    }

    window.addEventListener("resize", scheduleRefit);
    window.addEventListener("orientationchange", scheduleRefit);
    const viewport = window.visualViewport;
    viewport?.addEventListener("resize", scheduleRefit);
    viewport?.addEventListener("scroll", scheduleRefit);

    socket.addEventListener("open", () => {
      status = "connected";
      schedulePostLayoutRefit();
      requestAnimationFrame(() => term.focus());
    });

    const readMessageData = async (data: unknown): Promise<string> => {
      if (typeof data === "string") return data;
      if (data instanceof Blob) {
        if ("text" in data && typeof data.text === "function") return data.text();
        return new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.addEventListener("load", () => resolve(String(reader.result ?? "")));
          reader.addEventListener("error", () => reject(reader.error));
          reader.readAsText(data);
        });
      }
      if (data instanceof ArrayBuffer) return new TextDecoder().decode(data);
      return String(data);
    };

    socket.addEventListener("message", async (event) => {
      const data = await readMessageData(event.data);
      try {
        const payload = JSON.parse(data) as { type?: string; data?: string };
        if (payload.type === "output" && payload.data) {
          const binary = atob(payload.data);
          const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
          const decoded = outputDecoder.decode(bytes, { stream: true });
          term.write(decoded);
          if (pinnedToBottom) term.scrollToBottom();
          zerolag.clearFlushed();
          zerolag.rerender();
        } else if (payload.type === "error" && "error" in payload && payload.error) {
          status = String(payload.error);
        }
      } catch {
        term.write(data);
        if (pinnedToBottom) term.scrollToBottom();
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

      // Sticky Ctrl from the key bar: fold the next letter into its control code.
      if (ctrlArmed && data.length === 1) {
        ctrlArmed = false;
        const code = data.toLowerCase().charCodeAt(0);
        if (code >= 97 && code <= 122) {
          socket.send(JSON.stringify({ type: "input", data: String.fromCharCode(code - 96) }));
          return;
        }
      }

      if (data === "\r") {
        zerolag.clear();
        socket.send(JSON.stringify({ type: "input", data }));
        return;
      }

      if (data === "\x7f") {
        // Raw tmux/TUI attach: every keystroke is sent immediately, so the
        // remote PTY owns line editing. removeChar() only keeps the latency
        // overlay in sync — the backspace itself must always reach the PTY,
        // otherwise the deleted character lives on in the real buffer.
        zerolag.removeChar();
        socket.send(JSON.stringify({ type: "input", data }));
        return;
      }

      // Printable keystrokes are sent to the PTY immediately (raw attach), so
      // the zero-lag overlay tracks them as "flushed" — in flight, echo not yet
      // received — rather than "pending" (locally held, unsent). The output
      // handler's clearFlushed() wipes them the moment the echo lands, so the
      // overlay never accumulates text that already lives in the real buffer.
      if (data.length === 1 && data.charCodeAt(0) >= 32) {
        const { count, text } = zerolag.getFlushed();
        zerolag.setFlushed(count + 1, text + data);
      }

      socket.send(JSON.stringify({ type: "input", data }));
    });

    return () => {
      if (refitFrame) cancelAnimationFrame(refitFrame);
      resizeObserver?.disconnect();
      window.removeEventListener("resize", scheduleRefit);
      window.removeEventListener("orientationchange", scheduleRefit);
      viewport?.removeEventListener("resize", scheduleRefit);
      viewport?.removeEventListener("scroll", scheduleRefit);
      container?.removeEventListener("touchstart", onTouchStart);
      container?.removeEventListener("touchmove", onTouchMove);
      container?.removeEventListener("touchend", onTouchEnd);
      container?.removeEventListener("touchcancel", onTouchEnd);
      socket.close();
      zerolag.dispose();
      term.dispose();
    };
  });
</script>

<section class="terminal-panel" data-testid="task-terminal-panel" aria-label="Task terminal">
  <div class="terminal-host task-terminal-viewport" bind:this={container}></div>
  <div class="terminal-keys" role="toolbar" aria-label="Terminal keys">
    {#each CONTROL_KEYS as key (key.label)}
      <button
        type="button"
        class="terminal-key"
        onmousedown={(event) => event.preventDefault()}
        onclick={() => {
          sendKey(key.data);
          focusTerm();
        }}>{key.label}</button>
    {/each}
    <button
      type="button"
      class="terminal-key"
      class:is-armed={ctrlArmed}
      aria-pressed={ctrlArmed}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => {
        ctrlArmed = !ctrlArmed;
        focusTerm();
      }}>Ctrl</button>
  </div>
  {#if status !== "connected"}
    <div class="terminal-status" data-testid="terminal-status">{status}</div>
  {/if}
</section>

<style>
  .terminal-panel {
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
    .terminal-panel {
      height: min(58vh, 560px);
    }
  }

  @media (max-width: 767px) {
    .terminal-panel {
      margin-top: 8px;
    }
  }

  .terminal-host {
    flex: 1 1 auto;
    min-height: 0;
    padding: 8px;
  }

  .terminal-keys {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding: 6px 8px;
    border-top: 1px solid var(--rule);
    background: var(--paper);
  }

  .terminal-key {
    flex: none;
    min-width: 44px;
    min-height: 40px;
    padding: 6px 10px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--ink);
    font-family: var(--mono);
    font-size: 13px;
  }

  .terminal-key.is-armed {
    background: var(--teal-deep);
    border-color: var(--teal);
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
    max-width: 100%;
    min-width: 0;
  }

  :global(.terminal-panel .xterm-viewport) {
    max-width: 100%;
    overflow-y: auto;
    /* iOS momentum scrolling; contain so terminal-history scroll never chains
       out to the page (the root cause of the old "scrolling fights" bug). */
    -webkit-overflow-scrolling: touch;
    overscroll-behavior: contain;
  }

  :global(.terminal-panel .xterm-screen),
  :global(.terminal-panel .xterm-rows),
  :global(.terminal-panel .xterm-link-layer),
  :global(.terminal-panel .xterm-selection-layer),
  :global(.terminal-panel .xterm-text-layer) {
    max-width: 100%;
  }
</style>
