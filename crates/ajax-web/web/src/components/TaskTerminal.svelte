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

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let interactionEl: HTMLDivElement | undefined = $state();
  let spacerEl: HTMLDivElement | undefined = $state();
  let term: Terminal | undefined = $state();
  let connection: TerminalConnection | undefined = $state();
  let status = $state<TerminalConnectionStatus>("connecting");
  let statusDetail = $state("");
  let ctrlArmed = $state(false);
  let expanded = $state(false);
  let hasUnseenOutput = $state(false);

  const MIN_TERMINAL_COLS = 80;
  const RESIZE_DEBOUNCE_MS = 100;
  const EXPANDED_CLASS = "terminal-expanded";
  const FONT_STORAGE_KEY = "ajax.terminal.fontSize";
  const DEFAULT_FONT_SIZE = 13;
  const MIN_FONT_SIZE = 7;
  const MAX_FONT_SIZE = 20;
  const PINCH_ACTIVATION_PX = 12;

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

  let statusVisible = $derived(status !== "connected" || statusDetail.length > 0);
  let showReconnect = $derived(status === "reconnecting" || status === "unavailable");

  let resetResizeDedupe: (() => void) | undefined;
  let schedulePostLayoutRef: (() => void) | undefined;
  let jumpToBottomRef: (() => void) | undefined;

  function loadPersistedFontSize(): number {
    try {
      const raw = localStorage.getItem(FONT_STORAGE_KEY);
      if (!raw) return DEFAULT_FONT_SIZE;
      const size = Number(raw);
      if (!Number.isFinite(size) || size < MIN_FONT_SIZE || size > MAX_FONT_SIZE) {
        return DEFAULT_FONT_SIZE;
      }
      return size;
    } catch {
      return DEFAULT_FONT_SIZE;
    }
  }

  function persistFontSize(size: number) {
    try {
      localStorage.setItem(FONT_STORAGE_KEY, String(size));
    } catch {
      // Storage may be unavailable in private mode.
    }
  }

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

  const toggleExpanded = () => {
    expanded = !expanded;
    document.documentElement.classList.toggle(EXPANDED_CLASS, expanded);
    resetResizeDedupe?.();
    schedulePostLayoutRef?.();
  };

  onMount(() => {
    let fitAddon: FitAddon | undefined;
    let dataDisposable: { dispose(): void } | undefined;
    let scrollDisposable: { dispose(): void } | undefined;
    let resizeObserver: ResizeObserver | undefined;
    let lastSentCols = 0;
    let lastSentRows = 0;
    let resizeTimer: ReturnType<typeof setTimeout> | undefined;
    let fitFrame = 0;
    let followLive = true;
    let pinchStartDistance = 0;
    let pinchBaseFontSize = DEFAULT_FONT_SIZE;
    let pinchEngaged = false;
    let syncingScroll = false;
    let wrapperDroveScroll = false;

    const isKeyboardOpen = () => document.documentElement.classList.contains("keyboard-open");

    const resetDedupe = () => {
      lastSentCols = 0;
      lastSentRows = 0;
    };
    resetResizeDedupe = resetDedupe;

    const sendResizeNow = () => {
      if (!connection?.isOpen() || !term) return;
      if (isKeyboardOpen()) return;
      const cols = term.cols;
      const rows = term.rows;
      if (!Number.isInteger(cols) || !Number.isInteger(rows) || cols <= 0 || rows <= 0) return;
      if (cols === lastSentCols && rows === lastSentRows) return;
      lastSentCols = cols;
      lastSentRows = rows;
      connection.sendResize(cols, rows);
    };

    const clearTermScale = (termEl: HTMLElement) => {
      termEl.style.transform = "";
      termEl.style.transformOrigin = "";
      termEl.style.width = "";
      termEl.style.height = "";
    };

    const fitLocal = () => {
      if (!fitAddon || !term || !hostEl) return;
      const proposed = fitAddon.proposeDimensions();
      if (!proposed) return;
      if (
        !Number.isFinite(proposed.cols) ||
        !Number.isFinite(proposed.rows) ||
        !Number.isInteger(proposed.cols) ||
        !Number.isInteger(proposed.rows) ||
        proposed.cols <= 0 ||
        proposed.rows <= 0
      ) {
        return;
      }

      const termEl = term.element;
      if (!termEl) return;

      const hostWidth = hostEl.clientWidth;
      const hostHeight = hostEl.clientHeight;

      if (proposed.cols >= MIN_TERMINAL_COLS) {
        clearTermScale(termEl);
        if (term.cols !== proposed.cols || term.rows !== proposed.rows) {
          term.resize(proposed.cols, proposed.rows);
        }
        return;
      }

      const screenEl = termEl.querySelector<HTMLElement>(".xterm-screen");
      const cellWidth =
        screenEl && term.cols > 0 ? screenEl.offsetWidth / term.cols : hostWidth / proposed.cols;
      const cellHeight =
        screenEl && term.rows > 0 ? screenEl.offsetHeight / term.rows : hostHeight / proposed.rows;
      if (cellWidth <= 0 || cellHeight <= 0 || hostWidth <= 0 || hostHeight <= 0) return;

      const logicalCols = MIN_TERMINAL_COLS;
      const scale = Math.min(1, hostWidth / (logicalCols * cellWidth));
      const logicalRows = Math.max(1, Math.ceil(hostHeight / (cellHeight * scale)));

      term.resize(logicalCols, logicalRows);
      termEl.style.width = `${hostWidth / scale}px`;
      termEl.style.height = `${hostHeight / scale}px`;
      termEl.style.transformOrigin = "0 0";
      termEl.style.transform = `scale(${scale})`;
    };

    const scheduleFit = (resizeWithFit: boolean) => {
      if (fitFrame) cancelAnimationFrame(fitFrame);
      fitFrame = requestAnimationFrame(() => {
        fitFrame = 0;
        fitLocal();
        if (resizeWithFit) sendResizeNow();
      });
    };

    const scheduleImmediate = () => {
      scheduleFit(true);
    };

    const scheduleDebounced = () => {
      scheduleFit(false);
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        resizeTimer = undefined;
        sendResizeNow();
      }, RESIZE_DEBOUNCE_MS);
    };

    const schedulePostLayout = () => {
      scheduleImmediate();
      requestAnimationFrame(() => {
        scheduleImmediate();
      });
    };
    schedulePostLayoutRef = schedulePostLayout;

    const onViewportChange = () => scheduleDebounced();

    const touchDistance = (touches: TouchList) =>
      Math.hypot(touches[0].clientX - touches[1].clientX, touches[0].clientY - touches[1].clientY);

    const cellHeightPx = () => {
      if (!term || !interactionEl || term.rows <= 0) return 18;
      return Math.max(1, interactionEl.clientHeight / term.rows);
    };

    const scrollbackLines = () => {
      if (!term) return 0;
      return Math.max(0, term.buffer.active.length - term.rows);
    };

    const viewportTopLine = () => term?.buffer.active.viewportY ?? 0;

    const syncSpacer = () => {
      if (!term || !spacerEl || !interactionEl) return;
      spacerEl.style.height = `${scrollbackLines() * cellHeightPx()}px`;
    };

    const scrollInteractionToBottom = () => {
      if (!interactionEl) return;
      interactionEl.scrollTop = Math.max(0, interactionEl.scrollHeight - interactionEl.clientHeight);
    };

    const refreshFollow = () => {
      if (!interactionEl) return;
      const atBottom =
        interactionEl.scrollHeight <= interactionEl.clientHeight + 1 ||
        interactionEl.scrollTop + interactionEl.clientHeight >= interactionEl.scrollHeight - 1;
      followLive = atBottom;
      if (atBottom) hasUnseenOutput = false;
    };

    const syncWrapperFromTerm = () => {
      if (!term || !interactionEl) return;
      const maxTop = Math.max(0, interactionEl.scrollHeight - interactionEl.clientHeight);
      const linesUpFromBottom = Math.max(0, scrollbackLines() - viewportTopLine());
      const nextTop = Math.max(0, maxTop - linesUpFromBottom * cellHeightPx());
      if (Math.abs(interactionEl.scrollTop - nextTop) <= 1) {
        refreshFollow();
        return;
      }
      syncingScroll = true;
      interactionEl.scrollTop = nextTop;
      syncingScroll = false;
      refreshFollow();
    };

    const syncTermFromWrapper = () => {
      if (!term || !interactionEl) return;
      const maxTop = Math.max(0, interactionEl.scrollHeight - interactionEl.clientHeight);
      if (interactionEl.scrollTop < maxTop - 1) {
        followLive = false;
      }
      const fromBottomPx = Math.max(0, maxTop - interactionEl.scrollTop);
      const linesUpFromBottom = Math.floor(fromBottomPx / cellHeightPx());
      const targetLine = Math.max(0, term.buffer.active.length - term.rows - linesUpFromBottom);
      const clampedLine = Math.min(targetLine, Math.max(0, term.buffer.active.length - 1));
      if (viewportTopLine() === clampedLine) {
        refreshFollow();
        return;
      }
      syncingScroll = true;
      wrapperDroveScroll = true;
      term.scrollToLine(clampedLine);
      syncingScroll = false;
      wrapperDroveScroll = false;
      refreshFollow();
    };

    const applyOutput = () => {
      syncSpacer();
      if (followLive) {
        syncingScroll = true;
        term?.scrollToBottom();
        scrollInteractionToBottom();
        syncingScroll = false;
        refreshFollow();
      } else {
        hasUnseenOutput = true;
      }
    };

    const onInteractionScroll = () => {
      if (syncingScroll) return;
      syncTermFromWrapper();
    };

    const onTermScroll = () => {
      if (syncingScroll || wrapperDroveScroll) return;
      syncWrapperFromTerm();
    };

    const onInteractionClick = () => {
      term?.focus();
    };

    jumpToBottomRef = () => {
      followLive = true;
      hasUnseenOutput = false;
      syncingScroll = true;
      term?.scrollToBottom();
      scrollInteractionToBottom();
      syncingScroll = false;
      refreshFollow();
    };

    const onTouchStart = (event: TouchEvent) => {
      if (event.touches.length !== 2) {
        pinchStartDistance = 0;
        pinchEngaged = false;
        return;
      }
      if (event.cancelable) event.preventDefault();
      pinchEngaged = false;
      pinchStartDistance = touchDistance(event.touches);
      pinchBaseFontSize = term?.options.fontSize ?? DEFAULT_FONT_SIZE;
    };

    const onTouchMove = (event: TouchEvent) => {
      if (event.touches.length !== 2 || pinchStartDistance <= 0 || !term) return;
      if (event.cancelable) event.preventDefault();
      const distance = touchDistance(event.touches);
      if (!pinchEngaged && Math.abs(distance - pinchStartDistance) >= PINCH_ACTIVATION_PX) {
        pinchEngaged = true;
      }
      if (!pinchEngaged) return;
      const ratio = distance / pinchStartDistance;
      const next = Math.round(
        Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, pinchBaseFontSize * ratio)),
      );
      if (next !== term.options.fontSize) {
        term.options.fontSize = next;
        fitLocal();
      }
    };

    const onTouchEnd = () => {
      if (pinchStartDistance > 0 && pinchEngaged && term) {
        persistFontSize(term.options.fontSize ?? DEFAULT_FONT_SIZE);
        resetDedupe();
        schedulePostLayout();
      }
      pinchStartDistance = 0;
      pinchEngaged = false;
    };

    if (!hostEl || !interactionEl) {
      return;
    }

    if (typeof window.matchMedia !== "function") {
      return;
    }

    const initialFontSize = loadPersistedFontSize();
    const liveTerm = new Terminal({
      fontSize: initialFontSize,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      cursorBlink: true,
      scrollback: 2000,
      theme: {
        background: "#1c1714",
        foreground: "#f4eee0",
        cursor: "#52a095",
      },
    });
    fitAddon = new FitAddon();
    liveTerm.loadAddon(fitAddon);
    liveTerm.open(hostEl);
    term = liveTerm;
    fitLocal();
    syncSpacer();
    refreshFollow();

    scrollDisposable = liveTerm.onScroll(onTermScroll);
    interactionEl.addEventListener("scroll", onInteractionScroll, { passive: true });
    interactionEl.addEventListener("click", onInteractionClick);

    dataDisposable = liveTerm.onData((data) => sendKey(consumeCtrl(data)));

    interactionEl.addEventListener("touchstart", onTouchStart, { passive: false });
    interactionEl.addEventListener("touchmove", onTouchMove, { passive: false });
    interactionEl.addEventListener("touchend", onTouchEnd, { passive: true });
    interactionEl.addEventListener("touchcancel", onTouchEnd, { passive: true });

    connection = connectTaskTerminal(handle, {
      onOutput: (text) => {
        term?.write(text, applyOutput);
      },
      onServerError: (message) => {
        statusDetail = message;
      },
      onStatus: (next) => {
        status = next;
        if (next === "connected") {
          statusDetail = "";
        }
      },
      onOpen: () => {
        statusDetail = "";
        resetDedupe();
        scheduleImmediate();
      },
    });

    resizeObserver = new ResizeObserver(onViewportChange);
    resizeObserver.observe(hostEl);
    const panelEl = hostEl.parentElement;
    if (panelEl) resizeObserver.observe(panelEl);
    window.addEventListener("resize", onViewportChange);
    window.addEventListener("orientationchange", onViewportChange);
    const viewport = window.visualViewport;
    viewport?.addEventListener("resize", onViewportChange);

    return () => {
      dataDisposable?.dispose();
      scrollDisposable?.dispose();
      if (interactionEl) {
        interactionEl.removeEventListener("scroll", onInteractionScroll);
        interactionEl.removeEventListener("click", onInteractionClick);
        interactionEl.removeEventListener("touchstart", onTouchStart);
        interactionEl.removeEventListener("touchmove", onTouchMove);
        interactionEl.removeEventListener("touchend", onTouchEnd);
        interactionEl.removeEventListener("touchcancel", onTouchEnd);
      }
      if (ctrlTimer) clearTimeout(ctrlTimer);
      if (resizeTimer) clearTimeout(resizeTimer);
      if (fitFrame) cancelAnimationFrame(fitFrame);
      resizeObserver?.disconnect();
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("orientationchange", onViewportChange);
      viewport?.removeEventListener("resize", onViewportChange);
      document.documentElement.classList.remove(EXPANDED_CLASS);
      connection?.dispose();
      fitAddon?.dispose();
      term?.dispose();
      connection = undefined;
      term = undefined;
      resetResizeDedupe = undefined;
      schedulePostLayoutRef = undefined;
      jumpToBottomRef = undefined;
    };
  });
</script>

<section
  class="terminal-panel"
  class:is-expanded={expanded}
  data-testid="task-terminal-panel"
  aria-label="Task terminal">
  <div
    class="terminal-interaction-wrap"
    data-testid="terminal-interaction-surface"
    bind:this={interactionEl}>
    <div class="terminal-host" bind:this={hostEl}></div>
    <div class="terminal-scroll-spacer" bind:this={spacerEl} aria-hidden="true"></div>
    {#if hasUnseenOutput}
      <button
        type="button"
        class="terminal-new-output"
        onclick={() => jumpToBottomRef?.()}>New output ↓</button>
    {/if}
  </div>
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
    class:is-empty={!statusVisible}
    data-testid="terminal-status"
    aria-hidden={statusVisible ? "false" : "true"}>
    {#if statusVisible}
      <span class="terminal-status-label">{STATUS_LABELS[status]}</span>
      {#if statusDetail}
        <span class="terminal-status-detail">{statusDetail}</span>
      {/if}
      {#if showReconnect}
        <button
          type="button"
          class="terminal-status-reconnect"
          onclick={() => requestReconnect()}>Reconnect</button>
      {/if}
    {/if}
  </div>
  <div data-testid="terminal-bottom-controls">
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
  </div>
</section>

<style>
  .terminal-panel {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    margin-top: 12px;
    overflow: hidden;
  }

  .terminal-interaction-wrap {
    position: relative;
    flex: 1 1 auto;
    min-height: 120px;
    overflow-x: hidden;
    overflow-y: auto;
    overscroll-behavior: contain;
    touch-action: pan-y;
    width: 100%;
    background: #1c1714;
  }

  .terminal-host {
    position: sticky;
    top: 0;
    left: 0;
    width: 100%;
    min-height: 120px;
    overflow: hidden;
    background: #1c1714;
  }

  .terminal-scroll-spacer {
    width: 1px;
    pointer-events: none;
  }

  .terminal-host :global(.xterm),
  .terminal-host :global(.xterm-viewport),
  .terminal-host :global(.xterm-screen) {
    height: 100%;
    background: #1c1714;
  }

  .terminal-host :global(.xterm-viewport) {
    overflow: hidden !important;
  }

  .terminal-new-output {
    position: absolute;
    left: 50%;
    bottom: 8px;
    transform: translateX(-50%);
    z-index: 6;
    min-height: 32px;
    padding: 6px 12px;
    border: 1px solid var(--rule-strong);
    border-radius: 999px;
    background: var(--surface-raised);
    color: var(--ink);
    font-size: 12px;
    font-weight: 600;
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

  .terminal-expand-corner.is-armed {
    background: var(--teal-deep);
    border-color: var(--teal);
    color: var(--paper);
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

  @media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {
    .terminal-panel:not(.is-expanded) .terminal-interaction-wrap {
      height: 38vh;
    }

    .terminal-panel:not(.is-expanded) .terminal-host {
      height: 100%;
    }

    :global(html.terminal-expanded) .terminal-panel.is-expanded {
      position: fixed;
      top: 0;
      right: 0;
      left: 0;
      z-index: 45;
      display: flex;
      flex-direction: column;
      height: var(--app-band-height, 100dvh);
      min-height: 0;
      max-height: none;
      margin-top: 0;
      padding: env(safe-area-inset-top) 0 0;
      box-sizing: border-box;
      background: var(--paper);
      border-left: none;
      border-right: none;
      border-bottom: none;
      border-radius: 0;
    }

    .terminal-panel.is-expanded .terminal-interaction-wrap,
    .terminal-panel.is-expanded .terminal-host {
      min-height: 0;
    }
  }
</style>
