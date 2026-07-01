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
  // One of: connecting | connected | reconnecting | disconnected.
  let status = $state<"connecting" | "connected" | "reconnecting" | "disconnected">("connecting");
  let statusDetail = $state("");
  let ctrlArmed = $state(false);
  let hasUnseenOutput = $state(false);

  // Assigned inside onMount so the key bar can reach the live socket/terminal.
  let sendKey: (data: string) => void = () => {};
  let focusTerm: () => void = () => {};
  let jumpToBottom: () => void = () => {};
  let requestReconnect: () => void = () => {};

  const STATUS_LABELS: Record<typeof status, string> = {
    connecting: "Connecting…",
    connected: "Connected",
    reconnecting: "Reconnecting…",
    disconnected: "Disconnected",
  };

  // Control-key bar: keys the iOS keyboard lacks. The "Ctrl" button is a sticky
  // modifier folded into the next key — from the keyboard OR this bar — by
  // consumeCtrl below. It auto-expires so a forgotten arm can't silently
  // corrupt a later keystroke.
  const CONTROL_KEYS = [
    { label: "Esc", data: "\x1b" },
    { label: "Tab", data: "\t" },
    { label: "⌃C", data: "\x03" },
    { label: "←", data: "\x1b[D" },
    { label: "↑", data: "\x1b[A" },
    { label: "↓", data: "\x1b[B" },
    { label: "→", data: "\x1b[C" },
  ];

  // A sticky modifier the user forgot they armed would mangle the next thing
  // they type minutes later, so it auto-disarms after a short window.
  const CTRL_ARM_TIMEOUT_MS = 4000;
  let ctrlTimer: ReturnType<typeof setTimeout> | undefined;

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

  // Fold Ctrl into a key: letters become their control code, cursor keys become
  // the CSI form with the Ctrl modifier (param 5); anything else passes through.
  const controlModify = (data: string): string => {
    if (data.length === 1) {
      const code = data.toLowerCase().charCodeAt(0);
      if (code >= 97 && code <= 122) return String.fromCharCode(code - 96);
    }
    const cursor = /^\x1b\[([ABCD])$/.exec(data);
    if (cursor) return `\x1b[1;5${cursor[1]}`;
    return data;
  };

  // Apply an armed Ctrl to the next key from either input surface, consuming
  // the arm so it never affects a second key.
  const consumeCtrl = (data: string): string => {
    if (!ctrlArmed) return data;
    disarmCtrl();
    return controlModify(data);
  };

  // A phone-sized viewport needs a much larger cell than a desktop pane: 10px
  // was an unreadable squint on a Retina display and forced needless column
  // pressure. Prefer the media query; fall back to touch capability where
  // matchMedia is unavailable.
  const MOBILE_FONT_SIZE = 14;
  const DESKTOP_FONT_SIZE = 13;
  const isMobileViewport = (): boolean => {
    if (typeof window.matchMedia === "function") {
      return window.matchMedia("(max-width: 767px)").matches;
    }
    return (navigator.maxTouchPoints ?? 0) > 0;
  };

  onMount(() => {
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: "ui-monospace, SF Mono, Menlo, monospace",
      fontSize: isMobileViewport() ? MOBILE_FONT_SIZE : DESKTOP_FONT_SIZE,
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
      if (pinnedToBottom) hasUnseenOutput = false;
    });

    // Touch-drag scrolling. xterm 6 ships VS Code's touch-gesture code but
    // never wires it up, and its `.xterm-screen` overlays the scrollable
    // `.xterm-viewport`, so native touch scrolling never fires. Ajax owns touch
    // scrollback directly here instead of forwarding wheel events into tmux or
    // the foreground terminal app. A stationary tap still focuses the textarea
    // to type; only a drag scrolls, so scrolling no longer pops the iOS keyboard.
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
        term.scrollLines(step);
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

    // Reassigned on every (re)connect; the input/resize closures below read this
    // binding live, so they always target the current socket.
    let socket: WebSocket;

    // Mobile Safari drops the WebSocket whenever it backgrounds the tab, and a
    // failed tmux attach closes it too. A dead socket must recover on its own
    // (capped backoff) and immediately on foreground, rather than stranding the
    // user on a frozen pane with the word "closed".
    let reconnectAttempts = 0;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    let disposed = false;
    const RECONNECT_MAX_DELAY_MS = 15000;

    // The iOS keyboard animates the visual viewport shorter over several frames.
    // A tmux-attached client SIGWINCHes the shared window on every resize, so
    // spraying resize frames during that animation corrupts the pane. Detect the
    // keyboard from the viewport height gap and withhold server resizes while
    // it's open; the local fit still runs so xterm stays visually correct, and a
    // single resize is flushed once the viewport settles.
    const KEYBOARD_OPEN_THRESHOLD_PX = 150;
    const keyboardOpen = (): boolean => {
      const vp = window.visualViewport;
      return vp ? window.innerHeight - vp.height > KEYBOARD_OPEN_THRESHOLD_PX : false;
    };

    const sendResize = () => {
      if (socket.readyState !== WebSocket.OPEN) return;
      if (keyboardOpen()) return;
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
    jumpToBottom = () => {
      term.scrollToBottom();
      hasUnseenOutput = false;
      term.focus();
    };

    let refitFrame = 0;
    let viewportResizeTimer: ReturnType<typeof setTimeout> | undefined;

    const fitNow = () => {
      fitAddon.fit();
      if (pinnedToBottom) term.scrollToBottom();
    };

    // Connection-time refit: the PTY must learn the real size immediately (the
    // keyboard is never open at connect), so send without debouncing.
    const scheduleImmediateRefit = () => {
      if (refitFrame) cancelAnimationFrame(refitFrame);
      refitFrame = requestAnimationFrame(() => {
        fitNow();
        sendResize();
      });
    };

    // Event-driven refit (container/window/orientation/keyboard): fit locally
    // right away, but coalesce the server resize behind a debounce so a burst —
    // e.g. the keyboard animation — collapses into a single frame after things
    // settle (and is dropped entirely while the keyboard is open).
    const scheduleDebouncedRefit = () => {
      if (refitFrame) cancelAnimationFrame(refitFrame);
      refitFrame = requestAnimationFrame(fitNow);

      if (viewportResizeTimer) clearTimeout(viewportResizeTimer);
      viewportResizeTimer = setTimeout(() => {
        sendResize();
        viewportResizeTimer = undefined;
      }, 300);
    };

    const schedulePostLayoutRefit = () => {
      scheduleImmediateRefit();
      requestAnimationFrame(scheduleImmediateRefit);
    };

    const resizeObserver =
      typeof ResizeObserver !== "undefined" ? new ResizeObserver(scheduleDebouncedRefit) : null;
    if (container && resizeObserver) {
      resizeObserver.observe(container);
    }

    window.addEventListener("resize", scheduleDebouncedRefit);
    window.addEventListener("orientationchange", scheduleDebouncedRefit);
    const viewport = window.visualViewport;
    viewport?.addEventListener("resize", scheduleDebouncedRefit);
    viewport?.addEventListener("scroll", scheduleDebouncedRefit);

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

    const onSocketMessage = async (event: MessageEvent) => {
      const data = await readMessageData(event.data);
      try {
        const payload = JSON.parse(data) as { type?: string; data?: string };
        if (payload.type === "output" && payload.data) {
          const binary = atob(payload.data);
          const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
          const decoded = outputDecoder.decode(bytes, { stream: true });
          term.write(decoded);
          if (pinnedToBottom) {
            term.scrollToBottom();
          } else {
            hasUnseenOutput = true;
          }
          zerolag.clearFlushed();
          zerolag.rerender();
        } else if (payload.type === "error" && "error" in payload && payload.error) {
          statusDetail = String(payload.error);
        }
      } catch {
        term.write(data);
        if (pinnedToBottom) {
          term.scrollToBottom();
        } else {
          hasUnseenOutput = true;
        }
        zerolag.clearFlushed();
        zerolag.rerender();
      }
    };

    const scheduleReconnect = () => {
      if (disposed) return;
      status = "reconnecting";
      const delay = Math.min(RECONNECT_MAX_DELAY_MS, 1000 * 2 ** reconnectAttempts);
      reconnectAttempts += 1;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = setTimeout(() => {
        if (!disposed) connect();
      }, delay);
    };

    const reconnectNow = () => {
      if (disposed) return;
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = undefined;
      }
      reconnectAttempts = 0;
      status = "connecting";
      connect();
    };

    function connect() {
      socket = openTaskTerminalSocket(handle);
      socket.addEventListener("open", () => {
        // A successful open resets the backoff. A fresh tmux attach repaints the
        // pane and the resize-on-open makes tmux redraw at the real size, so no
        // explicit refresh frame is needed on reconnect.
        const isReconnect = reconnectAttempts > 0;
        reconnectAttempts = 0;
        statusDetail = "";
        status = "connected";
        // Keystrokes sent on the previous socket will never be echoed by this
        // new one, so drop the local input overlay; otherwise those characters
        // would linger as ghost text until the next output frame reconciled it.
        if (isReconnect) zerolag.clear();
        schedulePostLayoutRefit();
        requestAnimationFrame(() => term.focus());
      });
      socket.addEventListener("message", onSocketMessage);
      // An error is followed by close; let the close handler own reconnect so we
      // never schedule it twice.
      socket.addEventListener("error", () => {});
      socket.addEventListener("close", () => {
        if (disposed) return;
        scheduleReconnect();
      });
    }

    // Reconnect immediately when the tab returns to the foreground instead of
    // waiting out the backoff (mobile Safari kills the socket on background).
    const onVisibility = () => {
      if (
        document.visibilityState === "visible" &&
        (status === "reconnecting" || status === "disconnected")
      ) {
        reconnectNow();
      }
    };
    document.addEventListener("visibilitychange", onVisibility);

    // Exposed to the status banner's manual "Reconnect" button.
    requestReconnect = reconnectNow;

    connect();

    term.onData((data) => {
      if (socket.readyState !== WebSocket.OPEN) return;

      // Sticky Ctrl: fold it into this key (letter → control code, cursor key →
      // Ctrl-modified CSI) and consume the arm.
      if (ctrlArmed) {
        socket.send(JSON.stringify({ type: "input", data: consumeCtrl(data) }));
        return;
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
      disposed = true;
      if (refitFrame) cancelAnimationFrame(refitFrame);
      if (viewportResizeTimer) clearTimeout(viewportResizeTimer);
      if (ctrlTimer) clearTimeout(ctrlTimer);
      if (reconnectTimer) clearTimeout(reconnectTimer);
      document.removeEventListener("visibilitychange", onVisibility);
      resizeObserver?.disconnect();
      window.removeEventListener("resize", scheduleDebouncedRefit);
      window.removeEventListener("orientationchange", scheduleDebouncedRefit);
      viewport?.removeEventListener("resize", scheduleDebouncedRefit);
      viewport?.removeEventListener("scroll", scheduleDebouncedRefit);
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
  {#if hasUnseenOutput}
    <button
      type="button"
      class="terminal-new-output"
      onclick={() => {
        jumpToBottom();
      }}>New output ↓</button>
  {/if}
  <div class="terminal-keys" role="toolbar" aria-label="Terminal keys">
    {#each CONTROL_KEYS as key (key.label)}
      <button
        type="button"
        class="terminal-key"
        onmousedown={(event) => event.preventDefault()}
        onclick={() => {
          sendKey(consumeCtrl(key.data));
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
        toggleCtrl();
        focusTerm();
      }}>Ctrl{#if ctrlArmed}<span class="terminal-key-armed-dot" aria-hidden="true"></span>{/if}</button>
  </div>
  {#if status !== "connected"}
    <div class="terminal-status" data-testid="terminal-status">
      <span class="terminal-status-label">{STATUS_LABELS[status]}</span>
      {#if statusDetail}
        <span class="terminal-status-detail">{statusDetail}</span>
      {/if}
      {#if status === "reconnecting" || status === "disconnected"}
        <button
          type="button"
          class="terminal-status-reconnect"
          onclick={() => requestReconnect()}>Reconnect</button>
      {/if}
    </div>
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

  .terminal-new-output {
    align-self: center;
    min-height: 36px;
    margin: 0 8px 6px;
    padding: 6px 12px;
    border: 1px solid var(--teal);
    border-radius: var(--radius-sm);
    background: var(--teal-deep);
    color: var(--paper);
    font-size: 12px;
    font-weight: 700;
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
    color: var(--paper);
    animation: terminal-key-pulse 1s ease-in-out infinite;
  }

  /* Signals the armed modifier is live and temporary (it auto-expires). */
  @keyframes terminal-key-pulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 rgba(82, 160, 149, 0.6);
    }
    50% {
      box-shadow: 0 0 0 3px rgba(82, 160, 149, 0);
    }
  }

  .terminal-key-armed-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-left: 5px;
    border-radius: 50%;
    background: var(--paper);
    vertical-align: middle;
  }

  .terminal-status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-top: 1px solid var(--rule);
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .terminal-status-detail {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-transform: none;
    letter-spacing: normal;
  }

  .terminal-status-reconnect {
    flex: none;
    min-height: 32px;
    margin-left: auto;
    padding: 4px 12px;
    border: 1px solid var(--teal);
    border-radius: var(--radius-sm);
    background: var(--teal-deep);
    color: var(--paper);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
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
