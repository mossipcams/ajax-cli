<script lang="ts">
  import { flushSync, onMount } from "svelte";
  import { Ghostty, Terminal, FitAddon, type IDisposable } from "ghostty-web";
  import {
    connectTaskTerminal,
    type TerminalConnection,
    type TerminalConnectionStatus,
  } from "../terminalConnection";
  import { isKeyboardOpen } from "../viewport";
  import { attachTerminalGestures } from "../terminalGestures";
  import { createRefitScheduler } from "../terminalRefit";
  import {
    flooredCols,
    clampPan,
    fitCapFontSize,
    persistedFontSize,
    persistFontSize,
    persistedGeometryMode,
    persistGeometryMode,
    DEFAULT_FONT_SIZE,
    MAX_FONT_SIZE,
    MIN_TERMINAL_COLS,
    FIT_TERMINAL_COLS,
    type GeometryMode,
  } from "../terminalGeometry";
  const GHOSTTY_WASM_URL = "/ghostty-vt.wasm";
  const GHOSTTY_SCROLLBAR_RESERVATION_PX = 15;
  let ghosttyRuntime: Promise<Ghostty> | undefined;

  type TerminalWithRendererMetrics = Terminal & {
    renderer?: {
      getMetrics?: () => { width?: number; height?: number };
    };
  };

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
  let geometryMode = $state<GeometryMode>(persistedGeometryMode() ?? "fit");
  let hasUnseenOutput = $state(false);
  let expanded = $state(false);
  let zeroLagInput = $state("");
  let zeroLagStyle = $state("");

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
    let snapTimer: ReturnType<typeof setTimeout> | undefined;
    let snapFrames: number[] = [];

    const cancelExpandedSnap = () => {
      for (const frame of snapFrames) cancelAnimationFrame(frame);
      snapFrames = [];
      if (snapTimer) {
        clearTimeout(snapTimer);
        snapTimer = undefined;
      }
    };

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

    // The operator's font size — persisted pinch choice or the default. The
    // live font may sit below it: fitNow shrinks the rendered size whenever
    // the 80-column floor would overflow the host width, and climbs back
    // toward this choice when the viewport widens again.
    let chosenFontSize = persistedFontSize() ?? DEFAULT_FONT_SIZE;

    const colsFloor = () => (geometryMode === "wide" ? MIN_TERMINAL_COLS : FIT_TERMINAL_COLS);

    const cssPx = (style: CSSStyleDeclaration, property: string): number => {
      const value = Number.parseFloat(style.getPropertyValue(property));
      return Number.isFinite(value) ? value : 0;
    };

    // Largest font at which the column floor still fits the clipped host.
    // ghostty-web's FitAddon measures term.element, which is this same host;
    // after Ajax floors the grid to 80 cols that measurement may report the
    // current floor instead of the phone-visible width. Prefer the host's
    // visible clientWidth plus renderer cell metrics, and fall back to the
    // addon's proposal for pre-layout/jsdom cases.
    const hostWidthFitCap = (): number | undefined => {
      if (!container || !term) return undefined;
      const hostWidth = container.clientWidth;
      const cellWidth = (term as TerminalWithRendererMetrics).renderer?.getMetrics?.()?.width;
      const currentFont = term.options.fontSize ?? DEFAULT_FONT_SIZE;
      if (
        !Number.isFinite(hostWidth) ||
        hostWidth <= 0 ||
        !Number.isFinite(cellWidth) ||
        !cellWidth ||
        cellWidth <= 0 ||
        !Number.isFinite(currentFont) ||
        currentFont <= 0
      ) {
        return undefined;
      }
      const style = window.getComputedStyle(container);
      const usableWidth =
        hostWidth -
        cssPx(style, "padding-left") -
        cssPx(style, "padding-right") -
        GHOSTTY_SCROLLBAR_RESERVATION_PX;
      if (!Number.isFinite(usableWidth) || usableWidth <= 0) return undefined;
      return fitCapFontSize(
        currentFont,
        Math.floor(usableWidth / cellWidth),
        colsFloor(),
      );
    };

    const fitFontCap = (): number =>
      hostWidthFitCap() ??
      fitCapFontSize(
        term?.options.fontSize ?? DEFAULT_FONT_SIZE,
        fitAddon?.proposeDimensions()?.cols,
        colsFloor(),
      );

    // Touch/wheel scroll, horizontal pan, pinch-zoom, and momentum flings all
    // live in terminalGestures; the component only supplies the terminal-side
    // effects each gesture drives.
    const detachGestures = container
      ? attachTerminalGestures(container, {
          scrollLines: (lines) => {
            if (lines !== 0) {
              pinnedToBottom = false;
              cancelExpandedSnap();
            }
            term?.scrollLines(lines);
          },
          cellHeightPx,
          fontSize: () => term?.options.fontSize ?? DEFAULT_FONT_SIZE,
          maxFontSize: fitFontCap,
          setFontSize: (next) => {
            chosenFontSize = next;
            if (term) term.options.fontSize = next;
            persistFontSize(next);
            scheduleFontSizeRefit();
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

    // Fit rows to the container; the column floor depends on geometry mode.
    // Fit mode sizes the PTY to the visible width (40-col safety floor) so
    // phones get a readable grid without horizontal panning. Wide mode keeps
    // the classic 80-column floor: the hosted tmux/Claude Code TUI assumes
    // ~80, and a narrower PTY wraps nearly every line. When the floor exceeds
    // what fits, the font shrinks to keep every column on screen; only when
    // even the minimum font overflows does the canvas extend past the right
    // edge, with horizontal pan bringing it into view.
    let keyboardWasOpen = false;
    const clampHorizontalPan = () => {
      if (!container) return;
      container.scrollLeft = clampPan(container.scrollLeft, container.scrollWidth, container.clientWidth);
    };
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
      // Fit-to-width: the font tracks the operator's chosen size but shrinks
      // as far as the readable minimum so the 80-column floor fits the host —
      // a narrow screen must not hide half of every line off the right edge
      // (horizontal pan remains only for the sub-minimum overflow). Growth
      // back (rotating to a wider viewport) is one step per pass and stops a
      // pixel short of the cap: integer cell metrics make the cap jitter ±1
      // between adjacent font sizes, and a margin-less grow would oscillate
      // against the shrink rule.
      const currentFont = term.options.fontSize ?? DEFAULT_FONT_SIZE;
      const cap = hostWidthFitCap() ?? fitCapFontSize(currentFont, proposed?.cols, colsFloor());
      const growCeiling = cap >= MAX_FONT_SIZE ? cap : cap - 1;
      const grownFont = Math.min(chosenFontSize, growCeiling, currentFont + 1);
      const nextFont = currentFont > cap ? cap : Math.max(currentFont, grownFont);
      if (nextFont !== currentFont) {
        term.options.fontSize = nextFont;
        // Renderer cell metrics settle after a size change; refit next frame
        // so rows/cols and the server resize reflect the new font.
        scheduleDebouncedRefit();
      }
      if (proposed && Number.isFinite(proposed.rows) && proposed.rows > 0) {
        term.resize(flooredCols(proposed.cols, colsFloor()), proposed.rows);
      } else {
        // jsdom / pre-layout paints propose nothing; plain fit is the best guess.
        fitAddon.fit();
      }
      clampHorizontalPan();
      if (pinnedToBottom) term.scrollToBottom();
    };

    // When to fit and when to tell the PTY (frame coalescing, the resize
    // debounce, font-metric settling) is terminalRefit's policy; this
    // component only supplies the two effects. Stable wrappers because the
    // debounced one is also an event listener removed by identity.
    const refitScheduler = createRefitScheduler({ fit: fitNow, sendResize });
    const scheduleDebouncedRefit = () => refitScheduler.scheduleDebounced();
    const scheduleFontSizeRefit = () => refitScheduler.scheduleFontSize();
    const schedulePostLayoutRefit = () => refitScheduler.schedulePostLayout();
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
    // layout read. Ghostty scroll gestures cancel these pending frames so the
    // snap cannot fight deliberate scrollback reading.
    snapExpandedView = () => {
      cancelExpandedSnap();
      snapVisibleTerminal();
      const firstFrame = requestAnimationFrame(() => {
        snapFrames = snapFrames.filter((frame) => frame !== firstFrame);
        if (disposed) return;
        snapVisibleTerminal();
        const secondFrame = requestAnimationFrame(() => {
          snapFrames = snapFrames.filter((frame) => frame !== secondFrame);
          if (!disposed) snapVisibleTerminal();
        });
        snapFrames.push(secondFrame);
      });
      snapFrames.push(firstFrame);
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

    let optimisticPrintableAhead = "";
    let optimisticBackspacesAhead = 0;

    const writeOutput = (text: string) => {
      writeToTerminal(text);
      const pendingOptimistic = zeroLagInput;
      if (pendingOptimistic && text.includes(pendingOptimistic)) {
        optimisticPrintableAhead = "";
        optimisticBackspacesAhead = 0;
        zeroLagInput = "";
        zeroLagStyle = "";
      }
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
        optimisticPrintableAhead = "";
        optimisticBackspacesAhead = 0;
        zeroLagInput = "";
        zeroLagStyle = "";
        schedulePostLayoutRefit();
        requestAnimationFrame(() => term?.focus());
      },
    });

    // Exposed to the status banner's manual "Reconnect" button.
    requestReconnect = () => connection.reconnectNow();

    const cursorOverlayStyle = (): string => {
      const canvas = container?.querySelector<HTMLElement>("canvas");
      const active = term?.buffer.active as { cursorX?: number; cursorY?: number } | undefined;
      if (!canvas || !term || active?.cursorX === undefined || active.cursorY === undefined) {
        return "";
      }
      const cellWidth = canvas.clientWidth / term.cols;
      const cellHeight = canvas.clientHeight / term.rows;
      if (!Number.isFinite(cellWidth) || !Number.isFinite(cellHeight) || cellWidth <= 0 || cellHeight <= 0) {
        return "";
      }
      const left = Math.max(0, active.cursorX) * cellWidth;
      const top = Math.max(0, active.cursorY) * cellHeight;
      return `left: ${left}px; top: ${top}px;`;
    };

    const setZeroLagInput = (next: string) => {
      zeroLagInput = next;
      zeroLagStyle = next ? cursorOverlayStyle() : "";
    };

    const appendZeroLagInput = (data: string) => {
      flushSync(() => {
        setZeroLagInput(zeroLagInput + data);
      });
    };

    const trimZeroLagInput = () => {
      flushSync(() => {
        setZeroLagInput(zeroLagInput.slice(0, -1));
      });
    };

    const clearZeroLagInput = () => {
      flushSync(() => {
        optimisticPrintableAhead = "";
        optimisticBackspacesAhead = 0;
        setZeroLagInput("");
      });
    };

    const handleTextareaBeforeInput = (event: InputEvent) => {
      if (event.inputType === "insertText" && event.data) {
        optimisticPrintableAhead += event.data;
        appendZeroLagInput(event.data);
        return;
      }
      if (event.inputType === "deleteContentBackward") {
        optimisticBackspacesAhead += 1;
        trimZeroLagInput();
        return;
      }
      if (event.inputType === "insertLineBreak") {
        clearZeroLagInput();
      }
    };

    const consumeOptimisticPrintable = (data: string): boolean => {
      if (!optimisticPrintableAhead.startsWith(data)) return false;
      optimisticPrintableAhead = optimisticPrintableAhead.slice(data.length);
      return true;
    };

    const handleTerminalData = (raw: string) => {
      if (!connection.isOpen()) return;

      // Sticky Ctrl folds into this key (letter → control code, cursor key →
      // Ctrl-modified CSI). The folded byte then takes the normal branches, so
      // keys Ctrl leaves untouched (Enter, backspace) keep their overlay
      // bookkeeping instead of slipping past it.
      const data = consumeCtrl(raw);

      if (data === "\r") {
        clearZeroLagInput();
        connection.sendInput(data);
        return;
      }

      if (data === "\x7f") {
        if (optimisticBackspacesAhead > 0) {
          optimisticBackspacesAhead -= 1;
        } else {
          trimZeroLagInput();
        }
        connection.sendInput(data);
        return;
      }

      if (data.length === 1 && data.charCodeAt(0) >= 32) {
        if (!consumeOptimisticPrintable(data)) {
          appendZeroLagInput(data);
        }
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
      term.textarea?.addEventListener("beforeinput", handleTextareaBeforeInput);
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
      cancelExpandedSnap();
      refitScheduler.dispose();
      if (ctrlTimer) clearTimeout(ctrlTimer);
      connection.dispose();
      for (const subscription of terminalSubscriptions) subscription.dispose();
      term?.textarea?.removeEventListener("beforeinput", handleTextareaBeforeInput);
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
  <div class="terminal-host task-terminal-viewport" bind:this={container}>
    {#if zeroLagInput}
      <div
        class="terminal-zero-lag-input"
        data-testid="terminal-zero-lag-input"
        aria-hidden="true"
        style={zeroLagStyle}>
        {zeroLagInput}
      </div>
    {/if}
  </div>
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
        class:is-armed={geometryMode === "wide"}
        aria-pressed={geometryMode === "wide"}
        onmousedown={(event) => event.preventDefault()}
        onclick={() => {
          const next = geometryMode === "wide" ? "fit" : "wide";
          geometryMode = next;
          persistGeometryMode(next);
          refitAfterLayout();
          refocusTerm();
        }}>Wide</button>
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
    min-width: 0;
    max-width: 100%;
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
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    width: 100%;
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

  .terminal-zero-lag-input {
    position: absolute;
    z-index: 1;
    left: 8px;
    bottom: 8px;
    max-width: calc(100% - 16px);
    overflow: hidden;
    color: #f4eee0;
    font-family: ui-monospace, SF Mono, Menlo, monospace;
    font-size: 16px;
    line-height: 1.2;
    pointer-events: none;
    text-shadow: 0 0 6px #1c1714;
    white-space: pre;
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
