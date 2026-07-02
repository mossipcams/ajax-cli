<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { ZerolagInputAddon } from "xterm-zerolag-input";
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
  import "@xterm/xterm/css/xterm.css";

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
  let composerText = $state("");

  // Expanded mode hands the terminal the whole screen: on mobile the class
  // hides the task chrome (header/status/actions/details, see styles.css); on
  // desktop it lifts the panel into a fixed full-viewport overlay. The class
  // lives on <html> so page-level chrome outside this component can react.
  const EXPANDED_CLASS = "terminal-expanded";
  const setExpanded = (next: boolean) => {
    expanded = next;
    document.documentElement.classList.toggle(EXPANDED_CLASS, next);
  };

  // Assigned inside onMount so the key bar can reach the live socket/terminal.
  let sendKey: (data: string) => void = () => {};
  let focusTerm: () => void = () => {};
  let jumpToBottom: () => void = () => {};
  let requestReconnect: () => void = () => {};
  let requestPaste: () => void = () => {};
  let blurTerm: () => void = () => {};
  let refitAfterLayout: () => void = () => {};

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

  // Composer: batch a whole command locally, then submit it to the PTY as one
  // input (text + Enter). The raw terminal stays the default input surface.
  const sendComposer = () => {
    if (composerText.trim().length === 0) return;
    sendKey(`${composerText}\r`);
    composerText = "";
    focusTerm();
  };

  onMount(() => {
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: "ui-monospace, SF Mono, Menlo, monospace",
      fontSize: persistedFontSize() ?? DEFAULT_FONT_SIZE,
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

    const cellHeightPx = (): number => {
      const viewport = container?.querySelector<HTMLElement>(".xterm-viewport");
      const height = viewport?.clientHeight ?? 0;
      // jsdom and pre-layout paints report 0; fall back to a sane line height.
      return height > 0 && term.rows > 0 ? height / term.rows : 18;
    };

    // Touch/wheel scroll, horizontal pan, pinch-zoom, and momentum flings all
    // live in terminalGestures; the component only supplies the terminal-side
    // effects each gesture drives.
    const detachGestures = container
      ? attachTerminalGestures(container, {
          scrollLines: (lines) => term.scrollLines(lines),
          cellHeightPx,
          fontSize: () => term.options.fontSize ?? DEFAULT_FONT_SIZE,
          setFontSize: (next) => {
            term.options.fontSize = next;
            persistFontSize(next);
            scheduleDebouncedRefit();
          },
        })
      : () => {};

    // Declared before the refit closures that capture it; assigned once the
    // callbacks it feeds exist below.
    let connection: TerminalConnection;

    // The iOS keyboard animates the visual viewport shorter over several frames.
    // A tmux-attached client SIGWINCHes the shared window on every resize, so
    // spraying resize frames during that animation corrupts the pane. Withhold
    // server resizes while the keyboard is open (isKeyboardOpen — the same
    // hysteresis-guarded state that drives the CSS takeover, so the freeze and
    // the chrome collapse can never disagree); the local fit still runs so
    // xterm stays visually correct, and a single resize is flushed once the
    // viewport settles.
    const sendResize = () => {
      if (isKeyboardOpen()) return;
      connection.sendResize(term.cols, term.rows);
    };

    sendKey = (data: string) => connection.sendInput(data);
    focusTerm = () => term.focus();
    // iPhone keyboards can't dismiss themselves and the keyboard-open chrome
    // collapse hides the Back button; blurring xterm's textarea is the only
    // way to hand the screen back to the full-height terminal.
    blurTerm = () => term.blur();
    // Reading action only: focusing here would pop the iOS keyboard and
    // shrink the very output the user asked to see (same contract as expand).
    jumpToBottom = () => {
      term.scrollToBottom();
      hasUnseenOutput = false;
    };
    // iOS long-press paste doesn't reliably reach xterm's hidden textarea, so
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
          if (text) term.paste(text);
          pasteNotice = "";
          term.focus();
        })
        .catch(() => {
          pasteNotice = "Clipboard read failed — allow paste access and retry";
        });
    };

    let refitFrame = 0;
    let viewportResizeTimer: ReturnType<typeof setTimeout> | undefined;

    // Fit rows to the container but never let the PTY drop below 80 columns:
    // the hosted tmux/Claude Code TUI assumes ~80, and a narrower PTY wraps
    // nearly every line. When the floor exceeds what fits, the canvas extends
    // past the right edge and horizontal pan brings it into view.
    const fitNow = () => {
      if (isKeyboardOpen()) {
        // The server resize is withheld while the keyboard is open, so the
        // local grid must not change either: a grid smaller than the PTY makes
        // tmux cursor-address rows that no longer exist locally, and xterm
        // clamps those writes to its bottom row — the TUI input box drifts up
        // and overwrites the line below it. Keep grid == PTY and crop the
        // taller canvas bottom-anchored so the cursor/input row stays visible
        // above the keyboard.
        if (container) {
          container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
        }
        if (pinnedToBottom) term.scrollToBottom();
        return;
      }
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
    // Discrete layout jumps (the ⛶ expand toggle) refit through the immediate
    // path: waiting out the debounce leaves the grid misfit in the new space
    // for a visible beat.
    refitAfterLayout = schedulePostLayoutRefit;

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

    const writeOutput = (text: string) => {
      term.write(text);
      if (pinnedToBottom) {
        term.scrollToBottom();
      } else {
        hasUnseenOutput = true;
      }
      zerolag.clearFlushed();
      zerolag.rerender();
    };

    connection = connectTaskTerminal(handle, {
      onOutput: writeOutput,
      onServerError: (message) => {
        statusDetail = message;
      },
      onStatus: (next) => {
        status = next;
      },
      onOpen: (isReconnect) => {
        statusDetail = "";
        // Keystrokes sent on the previous socket will never be echoed by this
        // new one, so drop the local input overlay; otherwise those characters
        // would linger as ghost text until the next output frame reconciled it.
        if (isReconnect) zerolag.clear();
        schedulePostLayoutRefit();
        requestAnimationFrame(() => term.focus());
      },
    });

    // Exposed to the status banner's manual "Reconnect" button.
    requestReconnect = () => connection.reconnectNow();

    term.onData((raw) => {
      if (!connection.isOpen()) return;

      // Sticky Ctrl folds into this key (letter → control code, cursor key →
      // Ctrl-modified CSI). The folded byte then takes the normal branches, so
      // keys Ctrl leaves untouched (Enter, backspace) keep their overlay
      // bookkeeping instead of slipping past it.
      const data = consumeCtrl(raw);

      if (data === "\r") {
        zerolag.clear();
        connection.sendInput(data);
        return;
      }

      if (data === "\x7f") {
        // Raw tmux/TUI attach: every keystroke is sent immediately, so the
        // remote PTY owns line editing. removeChar() only keeps the latency
        // overlay in sync — the backspace itself must always reach the PTY,
        // otherwise the deleted character lives on in the real buffer.
        zerolag.removeChar();
        connection.sendInput(data);
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

      connection.sendInput(data);
    });

    return () => {
      setExpanded(false);
      if (refitFrame) cancelAnimationFrame(refitFrame);
      if (viewportResizeTimer) clearTimeout(viewportResizeTimer);
      if (ctrlTimer) clearTimeout(ctrlTimer);
      connection.dispose();
      resizeObserver?.disconnect();
      window.removeEventListener("resize", scheduleDebouncedRefit);
      window.removeEventListener("orientationchange", scheduleDebouncedRefit);
      viewport?.removeEventListener("resize", scheduleDebouncedRefit);
      viewport?.removeEventListener("scroll", scheduleDebouncedRefit);
      detachGestures();
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
  <div
    class="terminal-bottom-controls"
    data-testid="terminal-bottom-controls"
    aria-label="Terminal input controls">
    <form
      class="terminal-composer"
      data-testid="terminal-composer"
      aria-label="Terminal composer"
      onsubmit={(event) => {
        event.preventDefault();
        sendComposer();
      }}>
      <input
        class="terminal-composer-input"
        aria-label="Terminal input"
        autocomplete="off"
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
        bind:value={composerText}
        placeholder="Send command" />
      <button
        type="submit"
        class="terminal-composer-send"
        aria-label="Send terminal input"
        disabled={composerText.trim().length === 0}>Send</button>
    </form>
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
      <button
        type="button"
        class="terminal-key terminal-key-expand"
        class:is-armed={expanded}
        aria-label="Expand terminal"
        aria-pressed={expanded}
        onmousedown={(event) => event.preventDefault()}
        onclick={() => {
          setExpanded(!expanded);
          refitAfterLayout();
        }}>⛶</button>
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

  /* A landscape phone exceeds the width breakpoint but must not get the
     fixed desktop panel height — its takeover layout flex-fills instead. */
  @media (min-width: 768px) and (not ((pointer: coarse) and (max-height: 500px))) {
    .terminal-panel {
      height: min(58vh, 560px);
    }
  }

  @media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {
    .terminal-panel {
      margin-top: 8px;
    }

    .terminal-host {
      padding: 4px;
    }

    .terminal-keys {
      gap: 4px;
      padding: 3px 4px;
    }

    .terminal-key {
      min-height: 32px;
      padding: 2px 8px;
      font-size: 12px;
    }
  }

  .terminal-host {
    flex: 1 1 auto;
    min-height: 0;
    padding: 8px;
    /* The 80-column floor can make the xterm canvas wider than the phone
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

  .terminal-composer {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 38px;
  }

  .terminal-composer-input {
    flex: 1 1 auto;
    min-width: 0;
    height: 38px;
    padding: 7px 10px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: var(--paper-tint);
    color: var(--ink);
    font-family: var(--mono);
    font-size: 16px;
  }

  .terminal-composer-input:focus {
    border-color: var(--teal);
    outline: none;
  }

  .terminal-composer-send {
    flex: none;
    min-height: 38px;
    padding: 6px 12px;
    border: 1px solid var(--teal);
    border-radius: var(--radius-sm);
    background: var(--teal);
    color: var(--ink);
    font-size: 12px;
    font-weight: 700;
  }

  .terminal-composer-send:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .terminal-keys {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding: 0;
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
    min-width: 0;
  }

  :global(.terminal-panel .xterm-viewport) {
    /* Ajax owns 100% of scrollback: every touch and wheel gesture is
       intercepted and translated into term.scrollLines(). Native scrolling here
       is not just redundant — on iOS Safari the momentum-composited layer that
       -webkit-overflow-scrolling: touch created retained stale pixels when
       xterm rewrote row text, so one drag would native-scroll the layer AND
       scrollLines the buffer, desyncing them into duplicated/overwritten/
       unreadable rows. Disable native scrolling entirely so only scrollLines
       moves the view. touch-action:none stops iOS Safari from claiming the
       vertical drag as a native pan before our touchmove handler can. */
    overflow: hidden;
    overscroll-behavior: contain;
    touch-action: none;
  }

  /* No max-width clamps on .xterm-screen/.xterm-rows or the render layers:
     the renderer sizes them to cols × cellWidth, and with the 80-column floor
     they legitimately exceed the host width. The host's overflow:hidden +
     scrollLeft pan owns the clipping instead. */

  /* xterm 6 renders VS Code's 14px DOM scrollbar inside the terminal. On a
     phone it overlaps ~3 columns of text and flickers visible on every tmux
     redraw, while touch scrolling is entirely Ajax-owned (scrollLines) — so
     it is pure noise there. Desktop keeps it: it is proportionate and
     mouse-draggable. */
  @media (pointer: coarse) {
    :global(.terminal-panel .xterm-scrollable-element > .scrollbar) {
      display: none;
    }
  }
</style>
