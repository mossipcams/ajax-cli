<script lang="ts">
  import { onMount } from "svelte";
  import { WTerm } from "@wterm/dom";
  import { WasmBridge, type TerminalCore } from "@wterm/core";
  import "@wterm/dom/css";
  import {
    connectTaskTerminal,
    type TerminalConnection,
    type TerminalConnectionStatus,
  } from "../terminalConnection";
  import { createTerminalClipboardUi } from "../terminalClipboard";
  import { DEFAULT_FONT_SIZE, persistedFontSize, persistFontSize, pinchActivated, pinchFontSize, MIN_FONT_SIZE, MAX_FONT_SIZE } from "../terminalGeometry";
  import { isKeyboardOpen } from "../viewport";
  import { createRefitScheduler } from "../terminalRefit";
  import { createTerminalLayoutPolicy } from "../terminalLayoutPolicy";

  interface Props {
    handle: string;
    onInitFailure?: (message: string) => void;
  }

  let { handle, onInitFailure }: Props = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let term: WTerm | undefined = $state();
  let core: TerminalCore | undefined;
  let connection: TerminalConnection | undefined = $state();
  let status = $state<TerminalConnectionStatus>("connecting");
  let statusDetail = $state("");
  let ctrlArmed = $state(false);
  let scrolledUp = $state(false);
  let newOutput = $state(false);
  let expanded = $state(false);
  let layoutPolicy: ReturnType<typeof createTerminalLayoutPolicy> | undefined;

  const EXPANDED_CLASS = "terminal-expanded";
  const setExpanded = (next: boolean) => {
    expanded = next;
    document.documentElement.classList.toggle(EXPANDED_CLASS, next);
  };

  const toggleExpanded = () => {
    const next = !expanded;
    setExpanded(next);
    if (next) {
      layoutPolicy?.expandEnter();
      requestAnimationFrame(() => term?.focus());
    } else {
      layoutPolicy?.expandExit();
      (document.activeElement as HTMLElement | null)?.blur();
    }
  };

  const clipboardUi = createTerminalClipboardUi({ onChange: (snap) => (clipboard = snap) });
  let clipboard = $state(clipboardUi.snapshot());
  let pasteFallbackValue = $state("");

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
  const PINCH_ACTIVATION_PX = 12;
  let ctrlTimer: ReturnType<typeof setTimeout> | undefined;
  let currentFontSize = DEFAULT_FONT_SIZE;

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

  const applyCursorMode = (data: string): string => {
    const cursor = /^\x1b\[([ABCD])$/.exec(data);
    if (cursor && core?.cursorKeysApp()) return `\x1bO${cursor[1]}`;
    return data;
  };

  const encodePaste = (text: string): string => {
    if (!core?.bracketedPaste()) return text;
    // Strip ESC so clipboard text cannot close the bracketed-paste guard
    // and smuggle commands to the PTY (mirrors @wterm/dom's native paste).
    return `\x1b[200~${text.replace(/\x1b/g, "")}\x1b[201~`;
  };

  const isScrolledUp = (host: HTMLElement): boolean =>
    host.scrollHeight - host.scrollTop - host.clientHeight >= 5;
  const onHostScroll = () => {
    if (!hostEl) return;
    scrolledUp = isScrolledUp(hostEl);
    if (!scrolledUp) newOutput = false;
  };
  const snapToNewest = () => {
    if (!hostEl) return;
    hostEl.scrollTop = hostEl.scrollHeight;
    scrolledUp = false;
    newOutput = false;
  };

  const applyWtermTheme = (host: HTMLDivElement) => {
    const size = persistedFontSize() ?? DEFAULT_FONT_SIZE;
    currentFontSize = size;
    host.style.setProperty("--term-bg", "#1e1e1e");
    host.style.setProperty("--term-fg", "#d4d4d4");
    host.style.setProperty("--term-font-size", `${size}px`);
    host.style.setProperty("--term-color-0", "#1e1e1e");
  };

  const setWtermFontSize = (host: HTMLDivElement, size: number) => {
    currentFontSize = size;
    host.style.setProperty("--term-font-size", `${size}px`);
    persistFontSize(size);
  };

  const attachWtermPinchGestures = (host: HTMLDivElement): (() => void) => {
    const touchDistance = (touches: TouchList): number => {
      if (touches.length < 2) return 0;
      const dx = touches[0].clientX - touches[1].clientX;
      const dy = touches[0].clientY - touches[1].clientY;
      return Math.hypot(dx, dy);
    };

    let pinchStartDistance = 0;
    let pinchBaseFontSize = 0;
    let pinchEngaged = false;

    const resetPinch = () => {
      pinchStartDistance = 0;
      pinchEngaged = false;
    };

    const onTouchStart = (event: TouchEvent) => {
      if (event.touches.length === 2) {
        if (event.cancelable) event.preventDefault();
        pinchEngaged = false;
        pinchStartDistance = touchDistance(event.touches);
        pinchBaseFontSize = currentFontSize;
        return;
      }
      resetPinch();
    };

    const onTouchMove = (event: TouchEvent) => {
      if (event.touches.length !== 2 || pinchStartDistance <= 0) return;
      if (event.cancelable) event.preventDefault();
      const distance = touchDistance(event.touches);
      if (!pinchEngaged && pinchActivated(pinchStartDistance, distance, PINCH_ACTIVATION_PX)) {
        pinchEngaged = true;
      }
      if (!pinchEngaged) return;
      const next = pinchFontSize(
        pinchBaseFontSize,
        pinchStartDistance,
        distance,
        MIN_FONT_SIZE,
        MAX_FONT_SIZE,
      );
      if (next !== currentFontSize) setWtermFontSize(host, next);
    };

    const onTouchEnd = () => {
      resetPinch();
    };

    host.addEventListener("touchstart", onTouchStart, { passive: false });
    host.addEventListener("touchmove", onTouchMove, { passive: false });
    host.addEventListener("touchend", onTouchEnd);
    host.addEventListener("touchcancel", onTouchEnd);

    return () => {
      host.removeEventListener("touchstart", onTouchStart);
      host.removeEventListener("touchmove", onTouchMove);
      host.removeEventListener("touchend", onTouchEnd);
      host.removeEventListener("touchcancel", onTouchEnd);
    };
  };

  const sendKey = (data: string) => {
    if (!connection?.isOpen()) return;
    snapToNewest();
    connection.sendInput(data);
  };

  const refocusTerm = () => {
    // Only refocus when the terminal already owns focus: focusing from a
    // key-bar tap with the keyboard closed would pop the iOS keyboard.
    if (!hostEl?.contains(document.activeElement)) return;
    term?.focus();
  };

  const requestPaste = async () => {
    try {
      const text = await navigator.clipboard?.readText?.();
      if (text) {
        sendKey(encodePaste(text));
        return;
      }
      clipboardUi.openPasteFallback();
    } catch {
      // Clipboard denied — offer the manual tray instead of silence.
      clipboardUi.openPasteFallback();
    }
  };

  const sendPasteFallbackText = () => {
    const text = clipboardUi.takePasteFallbackText(pasteFallbackValue);
    pasteFallbackValue = "";
    if (!text) return;
    sendKey(encodePaste(text));
    term?.focus();
  };

  const requestReconnect = () => {
    connection?.reconnectNow();
  };

  onMount(() => {
    let disposed = false;
    let detachPinch: (() => void) | undefined;
    let keyboardObserver: MutationObserver | undefined;
    let refitScheduler: ReturnType<typeof createRefitScheduler> | undefined;
    let scheduleDebouncedRefit: (() => void) | undefined;
    const viewport = window.visualViewport;
    let wasKeyboardOpen = isKeyboardOpen();
    let lastCols = 1;
    let lastRows = 1;
    const policy = createTerminalLayoutPolicy();
    layoutPolicy = policy;

    const reportResize = (cols: number, rows: number) => {
      lastCols = Math.max(cols, 1);
      lastRows = Math.max(rows, 1);
      const open = isKeyboardOpen();
      const decision = policy.setKeyboardOpen(open);
      wasKeyboardOpen = open;
      if (!decision.allowPtyResize) return;
      connection?.sendResize(lastCols, lastRows);
    };

    const onKeyboardClassChange = () => {
      const open = isKeyboardOpen();
      if (!wasKeyboardOpen && open) {
        const decision = policy.setKeyboardOpen(true);
        if (decision.pinToBottomOnKeyboardOpen) {
          snapToNewest();
        }
      } else if (wasKeyboardOpen && !open) {
        policy.setKeyboardOpen(false);
        connection?.sendResize(lastCols, lastRows);
      }
      wasKeyboardOpen = open;
    };

    keyboardObserver = new MutationObserver(onKeyboardClassChange);
    keyboardObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    const init = async () => {
      if (!hostEl) return;
      try {
        core = await WasmBridge.load();
        if (disposed) return;

        applyWtermTheme(hostEl);

        const liveTerm = new WTerm(hostEl, {
          core,
          cols: 80,
          rows: 24,
          autoResize: false,
          onData: (data) => sendKey(consumeCtrl(data)),
          onResize: (cols, rows) => reportResize(cols, rows),
        });
        await liveTerm.init();
        if (disposed) {
          liveTerm.destroy();
          return;
        }

        term = liveTerm;
        hostEl.addEventListener("scroll", onHostScroll);
        detachPinch = attachWtermPinchGestures(hostEl);

        refitScheduler = createRefitScheduler({
          // Fixed 80x24 grid — no local resize on visualViewport jitter.
          fit: () => {},
          sendResize: () => reportResize(liveTerm.cols, liveTerm.rows),
        });
        scheduleDebouncedRefit = () => refitScheduler!.scheduleDebounced();
        viewport?.addEventListener("resize", scheduleDebouncedRefit);
        viewport?.addEventListener("scroll", scheduleDebouncedRefit);

        let hasOpened = false;
        const liveConnection = connectTaskTerminal(handle, {
          onOutput: (text) => {
            liveTerm.write(text);
            if (scrolledUp) newOutput = true;
          },
          onServerError: (message) => {
            statusDetail = message;
          },
          onStatus: (next) => {
            status = next;
          },
          onOpen: () => {
            statusDetail = "";
            if (hasOpened) {
              // Reconnect: clear the stale grid so the tmux repaint lands
              // clean, then re-announce our size to the PTY.
              liveTerm.write("\x1bc");
              reportResize(liveTerm.cols, liveTerm.rows);
            }
            hasOpened = true;
            requestAnimationFrame(() => liveTerm.focus());
          },
        });
        connection = liveConnection;
        reportResize(liveTerm.cols, liveTerm.rows);
        liveTerm.focus();
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        console.error("[ajax wterm] Surface V2 init failed:", error);
        try {
          sessionStorage.setItem("ajax.terminal.surfaceV2.lastError", message);
        } catch {
          // ignore
        }
        onInitFailure?.(message);
      }
    };

    void init();

    return () => {
      disposed = true;
      keyboardObserver?.disconnect();
      refitScheduler?.dispose();
      if (scheduleDebouncedRefit) {
        viewport?.removeEventListener("resize", scheduleDebouncedRefit);
        viewport?.removeEventListener("scroll", scheduleDebouncedRefit);
      }
      policy.dispose();
      layoutPolicy = undefined;
      setExpanded(false);
      if (ctrlTimer) clearTimeout(ctrlTimer);
      detachPinch?.();
      hostEl?.removeEventListener("scroll", onHostScroll);
      clipboardUi.dispose();
      connection?.dispose();
      term?.destroy();
      connection = undefined;
      term = undefined;
      core = undefined;
    };
  });
</script>

<div class="wterm-root">
  <section
    class="terminal-panel"
    class:is-expanded={expanded}
    data-testid="task-terminal-panel"
    data-terminal-engine="wterm"
    aria-label="Task terminal">
    <div class="terminal-host wterm-host" bind:this={hostEl}></div>
    {#if newOutput}
      <button type="button" class="terminal-new-output" onclick={() => snapToNewest()}>New output ↓</button>
    {/if}
    {#if clipboard.pasteFallbackOpen}
      <div class="terminal-paste-fallback" data-testid="terminal-paste-fallback">
        <textarea rows="3" bind:value={pasteFallbackValue} placeholder="Paste here, then Send"></textarea>
        <div class="terminal-paste-fallback-actions">
          <button type="button" class="terminal-key" onclick={() => sendPasteFallbackText()}>Send</button>
          <button
            type="button"
            class="terminal-key"
            onclick={() => {
              pasteFallbackValue = "";
              clipboardUi.closePasteFallback();
            }}>Cancel</button>
        </div>
      </div>
    {/if}
    <button
      type="button"
      class="terminal-expand-corner"
      class:is-armed={expanded}
      aria-label="Expand terminal"
      aria-pressed={expanded}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => toggleExpanded()}>⛶</button>
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
            sendKey(applyCursorMode(consumeCtrl(key.data)));
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
  .wterm-root {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    width: 100%;
  }

  .terminal-panel {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  .wterm-host {
    position: relative;
    flex: 1;
    min-height: 0;
    min-width: 0;
    width: 100%;
    height: auto !important;
    /* wterm scrolls on this element (has-scrollback → overflow-y). A shorthand
       overflow:hidden here beats .wterm.has-scrollback and kills scrollback. */
    overflow-x: hidden;
    overflow-y: auto;
    background: var(--term-bg, #1e1e1e);
  }

  :global(.wterm-host.wterm) {
    padding: 4px;
    box-shadow: none;
    border-radius: 0;
    /* Opaque fill — never inherit a transparent/composited wash on iOS. */
    background: var(--term-bg, #1e1e1e);
    color: var(--term-fg, #d4d4d4);
  }

  /* Keep the wterm hidden textarea invisible and 16px to avoid iOS focus-zoom
     painting a second visible text box while typing. */
  :global(.wterm-host.wterm textarea) {
    font-size: 16px;
    color: transparent;
    -webkit-text-fill-color: transparent;
    caret-color: transparent;
  }

  /*
   * @wterm/dom's Renderer copies the bottom-right cell's background onto
   * .term-grid as an INLINE style every render — with tmux's colored status
   * line as the bottom row, that washes the whole terminal yellow/green.
   * !important is required to beat the inline style.
   */
  :global(.wterm-host.wterm .term-grid) {
    background: var(--term-bg, #1e1e1e) !important;
  }

  :global(.wterm-host.wterm .term-row) {
    background: var(--term-bg, #1e1e1e) !important;
    box-shadow: none !important;
  }

  .terminal-keys {
    display: flex;
    flex-wrap: nowrap;
    gap: 4px;
    padding: 2px 4px;
    padding-bottom: max(2px, env(safe-area-inset-bottom));
    overflow-x: auto;
    scrollbar-width: none;
  }

  :global(html.keyboard-open) .terminal-keys {
    /* Keyboard covers the home indicator; safe-area pad is dead space. */
    padding-bottom: 6px;
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
    background: var(--paper-raised);
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
    background: var(--paper-raised);
    color: var(--ink);
  }

  .terminal-new-output {
    position: absolute;
    right: 12px;
    bottom: 8px;
    z-index: 2;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--rule);
    background: var(--paper-raised);
    color: var(--ink);
  }

  .terminal-expand-corner {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 5;
    min-width: 36px;
    min-height: 36px;
    padding: 4px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--paper) 72%, transparent);
    color: var(--ink-soft);
    font-size: 16px;
    line-height: 1;
  }

  .terminal-expand-corner:hover,
  .terminal-expand-corner:focus-visible {
    border-color: var(--ink-soft);
    color: var(--ink);
    outline: none;
  }

  .terminal-expand-corner.is-armed {
    top: calc(6px + env(safe-area-inset-top));
    right: calc(6px + env(safe-area-inset-right));
    background: var(--teal-deep);
    border-color: var(--teal);
    color: var(--paper);
  }

  .terminal-paste-fallback {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px 8px;
  }

  .terminal-paste-fallback textarea {
    font-family: var(--mono);
    font-size: 12px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--paper-raised);
    color: var(--ink);
    padding: 4px 6px;
  }

  .terminal-paste-fallback-actions {
    display: flex;
    gap: 4px;
  }
</style>
