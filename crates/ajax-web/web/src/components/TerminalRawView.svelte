<script lang="ts">
  import { onMount } from "svelte";
  import { Ghostty, Terminal, FitAddon, type IDisposable } from "ghostty-web";
  import {
    connectTaskTerminal,
    type TerminalConnection,
    type TerminalConnectionStatus,
  } from "../terminalConnection";
  import { isKeyboardOpen } from "../viewport";
  import { attachTerminalGestures } from "../terminalGestures";
  import {
    flooredCols,
    persistedFontSize,
    persistFontSize,
    DEFAULT_FONT_SIZE,
    MIN_TERMINAL_COLS,
  } from "../terminalGeometry";
  const GHOSTTY_WASM_URL = "/ghostty-vt.wasm";
  let ghosttyRuntime: Promise<Ghostty> | undefined;

  const loadGhosttyRuntime = () => {
    ghosttyRuntime ??= Ghostty.load(GHOSTTY_WASM_URL);
    return ghosttyRuntime;
  };

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  // A dead socket always auto-recovers (terminalConnection's backoff), so
  // there is no terminal "disconnected" state — only the reconnecting one.
  let status = $state<TerminalConnectionStatus>("connecting");
  let statusDetail = $state("");
  // Clipboard feedback is its own channel: paste outcomes must never clear or
  // overwrite a bridge-reported error in statusDetail.
  let pasteNotice = $state("");
  let ctrlArmed = $state(false);
  let hasUnseenOutput = $state(false);
  let expanded = $state(false);

  // Expanded mode gives the terminal a fixed visual-viewport layer. On mobile
  // that layer owns the PWA screen above the keyboard; on desktop it lifts the
  // panel into a full-viewport overlay. The class lives on <html> so page-level
  // chrome outside this component can react.
  const EXPANDED_CLASS = "terminal-expanded";
  const setExpanded = (next: boolean) => {
    expanded = next;
    document.documentElement.classList.toggle(EXPANDED_CLASS, next);
  };

  // Assigned inside onMount so the key bar can reach the live socket/terminal.
  let sendKey: (data: string) => void = () => {};
  let refocusTerm: () => void = () => {};
  let jumpToBottom: () => void = () => {};
  let requestReconnect: () => void = () => {};
  let requestPaste: () => void = () => {};
  let focusTerm: () => void = () => {};
  let blurTerm: () => void = () => {};
  let refitAfterLayout: () => void = () => {};
  let snapExpandedView: () => void = () => {};

  const STATUS_LABELS: Record<typeof status, string> = {
    connecting: "Connecting…",
    connected: "Connected",
    reconnecting: "Reconnecting…",
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

  onMount(() => {
    let disposed = false;
    let term: Terminal | undefined;
    let fitAddon: FitAddon | undefined;
    let connection: TerminalConnection;
    const terminalSubscriptions: IDisposable[] = [];
    const pendingOutput: string[] = [];

    // Auto-follow new output only while the user is at the bottom of the
    // scrollback. A tmux-attached session redraws constantly (status bar,
    // idle prompt refresh), and unconditionally calling scrollToBottom() on
    // every output frame yanked the view back down the instant a user tried
    // to scroll up — scrolling looked completely broken.
    let pinnedToBottom = true;

    const hardenMobileTextarea = () => {
      const input = term?.textarea;
      if (!input) return;
      input.setAttribute("autocapitalize", "off");
      input.setAttribute("autocorrect", "off");
      input.setAttribute("autocomplete", "off");
      input.setAttribute("spellcheck", "false");
      input.style.fontSize = "16px";
    };

    const cellHeightPx = (): number => {
      const viewport = container?.querySelector<HTMLElement>("canvas") ?? container;
      const height = viewport?.clientHeight ?? 0;
      // jsdom and pre-layout paints report 0; fall back to a sane line height.
      return height > 0 && term && term.rows > 0 ? height / term.rows : 18;
    };

    // Touch/wheel scroll, horizontal pan, pinch-zoom, and momentum flings all
    // live in terminalGestures; the component only supplies the terminal-side
    // effects each gesture drives.
    const detachGestures = container
      ? attachTerminalGestures(container, {
          scrollLines: (lines) => term?.scrollLines(lines),
          cellHeightPx,
          fontSize: () => term?.options.fontSize ?? DEFAULT_FONT_SIZE,
          setFontSize: (next) => {
            if (term) term.options.fontSize = next;
            persistFontSize(next);
            scheduleDebouncedRefit();
          },
        })
      : () => {};

    // The iOS keyboard animates the visual viewport shorter over several frames.
    // A tmux-attached client SIGWINCHes the shared window on every resize, so
    // spraying resize frames during that animation corrupts the pane. Withhold
    // server resizes while the keyboard is open (isKeyboardOpen — the same
    // hysteresis-guarded state that drives the CSS takeover, so the freeze and
    // the chrome collapse can never disagree); the local fit still runs so
    // Ghostty stays visually correct, and a single resize is flushed once the
    // viewport settles.
    const sendResize = () => {
      if (isKeyboardOpen()) return;
      if (!term) return;
      connection.sendResize(term.cols, term.rows);
    };

    sendKey = (data: string) => connection.sendInput(data);
    // Key-bar taps must never pop the iOS keyboard (a huge viewport jump) or
    // make Safari scroll-chase the hidden textarea: refocus only when the
    // terminal already owns focus, and without scrolling.
    refocusTerm = () => {
      const input = term?.textarea;
      if (!input || document.activeElement !== input) return;
      input.focus({ preventScroll: true });
    };
    focusTerm = () => term?.textarea?.focus({ preventScroll: true });
    // iPhone keyboards can't dismiss themselves; directly blurring the hidden
    // input is the reliable way to close the soft keyboard after fullscreen.
    blurTerm = () => {
      term?.textarea?.blur();
      term?.blur();
    };
    // Reading action only: focusing here would pop the iOS keyboard and
    // shrink the very output the user asked to see (same contract as expand).
    jumpToBottom = () => {
      term?.scrollToBottom();
      hasUnseenOutput = false;
    };
    // iOS long-press paste doesn't reliably reach the hidden terminal input, so
    // the key bar offers an explicit Paste. term.paste() honors bracketed-paste
    // mode and flows through the normal onData → socket path. Failures must be
    // visible: silently doing nothing reads as a broken button.
    requestPaste = () => {
      const clipboard = navigator.clipboard;
      if (!clipboard || typeof clipboard.readText !== "function") {
        pasteNotice = "Clipboard unavailable in this browser";
        return;
      }
      clipboard
        .readText()
        .then((text) => {
          if (text) term?.paste(text);
          pasteNotice = "";
          term?.focus();
        })
        .catch(() => {
          pasteNotice = "Clipboard read failed — allow paste access and retry";
        });
    };

    let refitFrame = 0;
    let viewportResizeTimer: ReturnType<typeof setTimeout> | undefined;
    let snapTimer: ReturnType<typeof setTimeout> | undefined;

    // Fit rows to the container but never let the PTY drop below 80 columns:
    // the hosted tmux/Claude Code TUI assumes ~80, and a narrower PTY wraps
    // nearly every line. When the floor exceeds what fits, the canvas extends
    // past the right edge and horizontal pan brings it into view.
    let keyboardWasOpen = false;
    const fitNow = () => {
      const keyboardOpen = isKeyboardOpen();
      if (keyboardOpen && !keyboardWasOpen) {
        // Opening the keyboard means the user is about to type: snap the view
        // to the cursor/input row even if they were reading scrollback, and
        // follow output from here on.
        pinnedToBottom = true;
        hasUnseenOutput = false;
      }
      keyboardWasOpen = keyboardOpen;
      if (keyboardOpen) {
        // The server resize is withheld while the keyboard is open, so the
        // local grid must not change either: a grid smaller than the PTY makes
        // tmux cursor-address rows that no longer exist locally, and the renderer
        // clamps those writes to its bottom row — the TUI input box drifts up
        // and overwrites the line below it. Keep grid == PTY and crop the
        // taller canvas bottom-anchored so the cursor/input row stays visible
        // above the keyboard.
        if (container) {
          container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
        }
        if (pinnedToBottom) term?.scrollToBottom();
        return;
      }
      if (!term || !fitAddon) return;
      if (container) container.scrollTop = 0;
      const proposed = fitAddon.proposeDimensions();
      if (proposed && Number.isFinite(proposed.rows) && proposed.rows > 0) {
        term.resize(flooredCols(proposed.cols, MIN_TERMINAL_COLS), proposed.rows);
      } else {
        // jsdom / pre-layout paints propose nothing; plain fit is the best guess.
        fitAddon.fit();
      }
      if (pinnedToBottom) term.scrollToBottom();
    };

    // Connection-time refit: the PTY must learn the real size immediately (the
    // keyboard is never open at connect), so send without debouncing.
    const scheduleImmediateRefit = () => {
      if (disposed) return;
      if (refitFrame) cancelAnimationFrame(refitFrame);
      refitFrame = requestAnimationFrame(() => {
        refitFrame = 0;
        if (disposed) return;
        fitNow();
        sendResize();
      });
    };

    // Event-driven refit (container/window/orientation/keyboard): fit locally
    // right away, but coalesce the server resize behind a debounce so a burst —
    // e.g. the keyboard animation — collapses into a single frame after things
    // settle (and is dropped entirely while the keyboard is open).
    const scheduleDebouncedRefit = () => {
      if (disposed) return;
      if (refitFrame) cancelAnimationFrame(refitFrame);
      refitFrame = requestAnimationFrame(() => {
        refitFrame = 0;
        if (disposed) return;
        fitNow();
      });

      if (viewportResizeTimer) clearTimeout(viewportResizeTimer);
      viewportResizeTimer = setTimeout(() => {
        sendResize();
        viewportResizeTimer = undefined;
      }, 300);
    };

    const schedulePostLayoutRefit = () => {
      scheduleImmediateRefit();
      requestAnimationFrame(() => {
        if (!disposed) scheduleImmediateRefit();
      });
    };
    // Discrete layout jumps (the ⛶ expand toggle) refit through the immediate
    // path: waiting out the debounce leaves the grid misfit in the new space
    // for a visible beat.
    refitAfterLayout = schedulePostLayoutRefit;

    const snapVisibleTerminal = () => {
      pinnedToBottom = true;
      hasUnseenOutput = false;
      document.documentElement.scrollTop = 0;
      document.body.scrollTop = 0;
      const scrollingElement = document.scrollingElement;
      if (scrollingElement) scrollingElement.scrollTop = 0;
      try {
        window.scrollTo(0, 0);
      } catch {
        // jsdom throws "Not implemented" for scrollTo.
      }
      if (container) {
        container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
      }
      term?.scrollToBottom();
    };

    // Fullscreen expand must land in the visible band above the iOS keyboard.
    // iOS moves the visual viewport over several frames, so repeat the snap
    // through the first part of that animation instead of trusting one early
    // layout read.
    snapExpandedView = () => {
      snapVisibleTerminal();
      requestAnimationFrame(() => {
        if (disposed) return;
        snapVisibleTerminal();
        requestAnimationFrame(() => {
          if (!disposed) snapVisibleTerminal();
        });
      });
      if (snapTimer) clearTimeout(snapTimer);
      snapTimer = setTimeout(() => {
        snapTimer = undefined;
        if (!disposed) snapVisibleTerminal();
      }, 260);
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

    const writeToTerminal = (text: string) => {
      if (!term) {
        pendingOutput.push(text);
        return;
      }
      term.write(text);
    };

    const writeOutput = (text: string) => {
      writeToTerminal(text);
      if (pinnedToBottom) {
        term?.scrollToBottom();
      } else {
        hasUnseenOutput = true;
      }
    };

    connection = connectTaskTerminal(handle, {
      onOutput: writeOutput,
      onServerError: (message) => {
        statusDetail = message;
      },
      onStatus: (next) => {
        status = next;
      },
      onOpen: () => {
        statusDetail = "";
        schedulePostLayoutRefit();
        requestAnimationFrame(() => term?.focus());
      },
    });

    // Exposed to the status banner's manual "Reconnect" button.
    requestReconnect = () => connection.reconnectNow();

    const handleTerminalData = (raw: string) => {
      if (!connection.isOpen()) return;

      // Sticky Ctrl folds into this key (letter → control code, cursor key →
      // Ctrl-modified CSI). The folded byte then takes the normal branches, so
      // keys Ctrl leaves untouched (Enter, backspace) keep their overlay
      // bookkeeping instead of slipping past it.
      const data = consumeCtrl(raw);

      if (data === "\r") {
        connection.sendInput(data);
        return;
      }

      if (data === "\x7f") {
        connection.sendInput(data);
        return;
      }

      connection.sendInput(data);
    };

    const flushPendingOutput = () => {
      if (!term) return;
      for (const text of pendingOutput) term.write(text);
      pendingOutput.length = 0;
      if (pinnedToBottom) term.scrollToBottom();
    };

    const mountGhosttyTerminal = async () => {
      const ghostty = await loadGhosttyRuntime();
      if (disposed || !container) return;
      fitAddon = new FitAddon();
      term = new Terminal({
        cursorBlink: true,
        fontFamily: "ui-monospace, SF Mono, Menlo, monospace",
        fontSize: persistedFontSize() ?? DEFAULT_FONT_SIZE,
        ghostty,
        theme: {
          background: "#1c1714",
          foreground: "#f4eee0",
          cursor: "#52a095",
        },
      });
      term.loadAddon(fitAddon);
      term.open(container);
      hardenMobileTextarea();
      terminalSubscriptions.push(
        term.onScroll(() => {
          pinnedToBottom = term ? term.getViewportY() <= 0 : true;
          if (pinnedToBottom) hasUnseenOutput = false;
        }),
        term.onData(handleTerminalData),
      );
      flushPendingOutput();
      schedulePostLayoutRefit();
      requestAnimationFrame(() => term?.focus());
    };

    void mountGhosttyTerminal().catch((error) => {
      statusDetail = error instanceof Error ? error.message : String(error);
    });

    return () => {
      disposed = true;
      setExpanded(false);
      if (refitFrame) cancelAnimationFrame(refitFrame);
      if (viewportResizeTimer) clearTimeout(viewportResizeTimer);
      if (snapTimer) clearTimeout(snapTimer);
      if (ctrlTimer) clearTimeout(ctrlTimer);
      connection.dispose();
      for (const subscription of terminalSubscriptions) subscription.dispose();
      resizeObserver?.disconnect();
      window.removeEventListener("resize", scheduleDebouncedRefit);
      window.removeEventListener("orientationchange", scheduleDebouncedRefit);
      viewport?.removeEventListener("resize", scheduleDebouncedRefit);
      viewport?.removeEventListener("scroll", scheduleDebouncedRefit);
      detachGestures();
      fitAddon?.dispose();
      term?.dispose();
    };
  });
</script>

<section
  class="terminal-panel"
  data-testid="task-terminal-panel"
  data-terminal-engine="ghostty"
  aria-label="Task terminal">
  <div class="terminal-host task-terminal-viewport" bind:this={container}></div>
  <button
    type="button"
    class="terminal-expand-corner"
    class:is-armed={expanded}
    aria-label="Expand terminal"
    aria-pressed={expanded}
    onmousedown={(event) => event.preventDefault()}
    onclick={() => {
      const next = !expanded;
      setExpanded(next);
      if (next) {
        focusTerm();
        snapExpandedView();
      } else {
        blurTerm();
      }
      refitAfterLayout();
    }}>⛶</button>
  {#if hasUnseenOutput}
    <button
      type="button"
      class="terminal-new-output"
      onclick={() => {
        jumpToBottom();
      }}>New output ↓</button>
  {/if}
  <div
    class="terminal-bottom-controls"
    data-testid="terminal-bottom-controls"
    aria-label="Terminal input controls">
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
        onclick={() => requestPaste()}>Paste</button>
      <button
        type="button"
        class="terminal-key"
        aria-label="Hide keyboard"
        onclick={() => blurTerm()}>⌄</button>
    </div>
  </div>
  {#if status !== "connected" || statusDetail || pasteNotice}
    <div class="terminal-status" data-testid="terminal-status">
      <span class="terminal-status-label">{STATUS_LABELS[status]}</span>
      {#if statusDetail}
        <span class="terminal-status-detail">{statusDetail}</span>
      {/if}
      {#if pasteNotice}
        <span class="terminal-status-detail">{pasteNotice}</span>
      {/if}
      {#if status === "reconnecting"}
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
    position: relative;
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

  /* Fullscreen toggle pinned to the terminal's top-right corner. Translucent
     over the canvas; it both enters and exits the expanded takeover, so it must
     stay visible in either state. */
  .terminal-expand-corner {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 2;
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
    background: var(--teal-deep);
    border-color: var(--teal);
    color: var(--paper);
  }

  /* A landscape phone exceeds the width breakpoint but must not get the
     fixed desktop panel height — its takeover layout flex-fills instead. */
  @media (min-width: 768px) and (not ((pointer: coarse) and (max-height: 500px))) {
    .terminal-panel {
      height: min(58vh, 560px);
    }
  }

  @media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {
    /* Full-bleed: the task page drops its horizontal padding on mobile so the
       terminal runs edge to edge; side/bottom borders and radii would read as
       stray hairlines against the screen edges. */
    .terminal-panel {
      margin-top: 8px;
      border-left: none;
      border-right: none;
      border-bottom: none;
      border-radius: 0;
    }

    .terminal-host {
      padding: 4px;
    }

    .terminal-keys {
      gap: 4px;
      padding: 2px 4px;
    }

    .terminal-key {
      min-height: 28px;
      padding: 1px 7px;
      font-size: 11px;
    }
  }

  .terminal-host {
    flex: 1 1 auto;
    min-height: 0;
    padding: 8px;
    /* The 80-column floor can make the Ghostty canvas wider than the phone
       viewport. The host clips it and the touch handler pans it via
       scrollLeft (programmatic scrolling works on overflow:hidden boxes);
       nothing else may scroll this element. */
    overflow: hidden;
    /* Ajax synthesizes 100% of scrolling from touch drags via
       term.scrollLines() (see the touchmove handler). Without touch-action:none
       iOS Safari latches a native pan in the first pixels of a vertical drag —
       before the handler's threshold fires preventDefault() — then delivers the
       rest of the gesture as non-cancelable touchmoves. That native pan (which
       has nothing to move: every scroll container is overflow:hidden) races and
       beats scrollLines(), so the terminal appears completely unscrollable on
       touch. none keeps every touchmove cancelable and hands the whole gesture
       to our handler. */
    touch-action: none;
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

  .terminal-bottom-controls {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 8px;
    padding-bottom: max(6px, env(safe-area-inset-bottom));
    border-top: 1px solid var(--rule);
    background: var(--paper);
  }

  .terminal-keys {
    display: flex;
    gap: 4px;
    overflow-x: auto;
    padding: 2px 4px;
    background: var(--paper);
  }

  .terminal-key {
    flex: none;
    min-width: 38px;
    min-height: 28px;
    padding: 3px 7px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--ink);
    font-family: var(--mono);
    font-size: 11px;
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

  :global(.terminal-panel canvas) {
    display: block;
    height: 100%;
    min-width: 0;
  }
</style>
