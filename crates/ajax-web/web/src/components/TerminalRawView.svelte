<script lang="ts">
  import { flushSync, onMount } from "svelte";
  import { Ghostty, Terminal, FitAddon, type IDisposable } from "ghostty-web";
  import {
    connectTaskTerminal,
    type TerminalConnection,
    type TerminalConnectionStatus,
  } from "../terminalConnection";
  import { isKeyboardOpen, resetDocumentScroll } from "../viewport";
  import { attachTerminalGestures } from "../terminalGestures";
  import { createRefitScheduler } from "../terminalRefit";
  import { cellAtPoint, orderedSelection, type CellPoint } from "../terminalGestures";
  import { copyText } from "../diagnostics";
  import {
    flooredCols,
    clampPan,
    fitCapFontSize,
    persistedFontSize,
    persistFontSize,
    terminalScrollbackLines,
    DEFAULT_FONT_SIZE,
    MAX_FONT_SIZE,
    FIT_TERMINAL_COLS,
  } from "../terminalGeometry";
  import {
    scrollbackGrowthCompensation,
    outputFollowEffects,
    validTerminalSize,
    createResizeDedupe,
    createTerminalWriteBatcher,
  } from "../terminalOutputPolicy";
  const GHOSTTY_WASM_URL = "/ghostty-vt.wasm";
  const TERMINAL_PLACEHOLDER_KEY = "ajax.debug.terminalPlaceholder";
  const placeholderMode =
    typeof localStorage !== "undefined" &&
    localStorage.getItem(TERMINAL_PLACEHOLDER_KEY) === "true";
  let ghosttyRuntime: Promise<Ghostty> | undefined;

  type TerminalWithRendererMetrics = Terminal & {
    renderer?: {
      getMetrics?: () => { width?: number; height?: number };
    };
  };

  // ghostty-web's public select() stores rows in the wrong coordinate space
  // (viewportY + row, where its own getSelection()/renderer read
  // scrollbackLength + row − viewportY), so Ajax writes the range through the
  // selection manager directly in the space the library reads back. A
  // standalone type (not a Terminal intersection) because these members are
  // private on the class; the cast goes through unknown.
  type SelectionEndpoint = { col: number; absoluteRow: number };
  type TerminalSelectionInternals = {
    showScrollbar?: () => void;
    scrollToBottom?: () => void;
    getScrollbackLength?: () => number;
    selectionManager?: {
      selectionStart: SelectionEndpoint | null;
      selectionEnd: SelectionEndpoint | null;
      requestRender?: () => void;
    };
  };
  const terminalInternals = (candidate: Terminal | undefined): TerminalSelectionInternals =>
    (candidate ?? {}) as unknown as TerminalSelectionInternals;

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
  let inlineSpacerHeight = $state(0);
  // Insecure (plain-http LAN) origins have no async clipboard API, so the
  // Paste key falls back to a real textarea the iOS Paste menu can target.
  let pasteFallbackOpen = $state(false);
  let pasteFallbackInput = $state<HTMLTextAreaElement | undefined>();
  const openPasteFallback = () => {
    pasteFallbackOpen = true;
  };
  const closePasteFallback = () => {
    pasteFallbackOpen = false;
  };
  $effect(() => {
    if (pasteFallbackOpen) pasteFallbackInput?.focus();
  });
  let copyOverlayOpen = $state(false);
  let copyOverlayText = $state("");
  let copyFallbackOpen = $state(false);
  let copyFallbackInput = $state<HTMLTextAreaElement | undefined>();
  $effect(() => {
    if (copyFallbackOpen) {
      copyFallbackInput?.focus();
      copyFallbackInput?.select();
    }
  });
  let zeroLagInput = $state("");
  let zeroLagStyle = $state("");

  // Expanded mode gives the terminal a fixed visual-viewport layer. On mobile
  // that layer owns the PWA screen above the keyboard; on desktop it lifts the
  // panel into a full-viewport overlay. The class lives on <html> so page-level
  // chrome outside this component can react.
  const EXPANDED_CLASS = "terminal-expanded";
  const setExpanded = (next: boolean) => {
    if (next) {
      const panel = container?.closest("[data-testid='task-terminal-panel']") as HTMLElement | null;
      const measured = Math.round(panel?.getBoundingClientRect().height ?? 0);
      inlineSpacerHeight = measured > 0 ? measured : 280;
    } else {
      inlineSpacerHeight = 0;
    }
    expanded = next;
    document.documentElement.classList.toggle(EXPANDED_CLASS, next);
  };

  // Assigned inside onMount so the key bar can reach the live socket/terminal.
  let sendKey: (data: string) => void = () => {};
  let pasteToTerm: (text: string) => void = () => {};
  const sendPasteFallbackText = (text: string) => {
    closePasteFallback();
    if (pasteFallbackInput) pasteFallbackInput.value = "";
    if (text) pasteToTerm(text);
  };
  let refocusTerm: () => void = () => {};
  let jumpToBottom: () => void = () => {};
  let requestReconnect: () => void = () => {};
  let requestPaste: () => void = () => {};
  let handleCopyOverlay: () => void | Promise<void> = () => {};
  let clearTermSelection: () => void = () => {};
  let focusTerm: () => void = () => {};
  let blurTerm: () => void = () => {};
  let refitAfterLayout: () => void = () => {};
  let snapExpandedView: () => void = () => {};
  let beginExpandFlush: () => void = () => {};

  const STATUS_LABELS: Record<typeof status, string> = {
    connecting: "Connecting…",
    connected: "Connected",
    reconnecting: "Reconnecting…",
    unavailable: "No live session",
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
    if (placeholderMode) {
      status = "connected";
      return () => {
        setExpanded(false);
        if (ctrlTimer) clearTimeout(ctrlTimer);
      };
    }

    let disposed = false;
    let term: Terminal | undefined;
    let fitAddon: FitAddon | undefined;
    let connection: TerminalConnection;
    const resizeDedupe = createResizeDedupe((cols, rows) => {
      connection.sendResize(cols, rows);
    });
    const terminalSubscriptions: IDisposable[] = [];
    const pendingOutput: string[] = [];

    // Auto-follow new output only while the user is at the bottom of the
    // scrollback. A tmux-attached session redraws constantly (status bar,
    // idle prompt refresh), and unconditionally calling scrollToBottom() on
    // every output frame yanked the view back down the instant a user tried
    // to scroll up — scrolling looked completely broken.
    let pinnedToBottom = true;
    let snapTimer: ReturnType<typeof setTimeout> | undefined;
    let expandFlushTimer: ReturnType<typeof setTimeout> | undefined;
    let snapFrames: number[] = [];

    // ghostty-web 0.4's write path force-scrolls to the bottom whenever the
    // viewport is away from it (writeInternal ends with
    // `viewportY !== 0 && scrollToBottom()`), which yanks the user out of
    // scrollback on every output frame — on a busy task scrolling up looks
    // completely dead. Ajax owns the follow-output policy (pinnedToBottom),
    // so mountGhosttyTerminal blinds the instance method and keeps the real
    // one here for Ajax's intentional bottom snaps.
    let libraryScrollToBottom: (() => void) | undefined;
    const snapScrollbackToBottom = () => libraryScrollToBottom?.();

    const scrollbackLines = (): number =>
      terminalInternals(term).getScrollbackLength?.() ?? 0;

    const cancelExpandedSnap = () => {
      for (const frame of snapFrames) cancelAnimationFrame(frame);
      snapFrames = [];
      if (snapTimer) {
        clearTimeout(snapTimer);
        snapTimer = undefined;
      }
    };

    // iOS hold-to-delete needs deletable textarea content; without a sentinel
    // the soft keyboard never starts its beforeinput repeat loop.
    const BACKSPACE_SENTINEL = "\u200B";

    const seedBackspaceSentinel = (input: HTMLTextAreaElement) => {
      if (!input.value.includes(BACKSPACE_SENTINEL)) {
        input.value = BACKSPACE_SENTINEL;
      }
    };

    const handleTextareaFocusForBackspaceSentinel = () => {
      const input = term?.textarea;
      if (input) seedBackspaceSentinel(input);
    };

    const hardenMobileTextarea = () => {
      const input = term?.textarea;
      if (!input) return;
      input.setAttribute("autocapitalize", "off");
      input.setAttribute("autocorrect", "off");
      input.setAttribute("autocomplete", "off");
      input.setAttribute("spellcheck", "false");
      input.style.fontSize = "16px";
      // ghostty clips the textarea to a fully invisible 1px box (opacity:0 +
      // clipPath:inset(50%)). Soften just enough that iOS treats it as a real
      // edit target for native Paste. Keep the element slightly opaque/unclipped
      // for paste targeting, but make text/caret paint transparent so typed
      // characters do not echo beside the canvas.
      input.style.opacity = "0.01";
      input.style.setProperty("clip-path", "none");
      input.style.setProperty("-webkit-clip-path", "none");
      input.style.setProperty("clip", "auto");
      input.style.color = "transparent";
      input.style.setProperty("-webkit-text-fill-color", "transparent");
      input.style.caretColor = "transparent";
      seedBackspaceSentinel(input);
      input.addEventListener("focus", handleTextareaFocusForBackspaceSentinel);
    };

    const cellHeightPx = (): number => {
      const viewport = container?.querySelector<HTMLElement>("canvas:not([aria-hidden='true'])") ?? container;
      const height = viewport?.clientHeight ?? 0;
      // jsdom and pre-layout paints report 0; fall back to a sane line height.
      return height > 0 && term && term.rows > 0 ? height / term.rows : 18;
    };

    // The operator's font size — persisted pinch choice or the default. The
    // live font may sit below it when the column floor would overflow the host
    // width, and climbs back toward this choice when the viewport widens again.
    let chosenFontSize = persistedFontSize() ?? DEFAULT_FONT_SIZE;

    const colsFloor = () => FIT_TERMINAL_COLS;

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
        hostWidth - cssPx(style, "padding-left") - cssPx(style, "padding-right");
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

    // ghostty-web's FitAddon reserves 15px of width for the scrollbar Ajax
    // suppresses (see mountGhosttyTerminal), so its column proposal stops a
    // couple of cells short of the screen edge. Fit columns from the full
    // host width instead; the addon's proposal remains the pre-layout/jsdom
    // fallback.
    const hostFitCols = (): number | undefined => {
      if (!container || !term) return undefined;
      const cellWidth = (term as TerminalWithRendererMetrics).renderer?.getMetrics?.()?.width;
      const width = container.clientWidth;
      if (
        !Number.isFinite(width) ||
        width <= 0 ||
        !cellWidth ||
        !Number.isFinite(cellWidth) ||
        cellWidth <= 0
      ) {
        return undefined;
      }
      return Math.floor(width / cellWidth);
    };

    // Long-press copy selection. The range lives in ghostty-web's
    // SelectionManager so its renderer paints the highlight; Ajax maps touch
    // points to cells (terminalSelection) and writes the endpoints directly
    // (see TerminalWithSelectionInternals for why select() is bypassed).
    let selectionAnchor: CellPoint | undefined;
    let copyNoticeTimer: ReturnType<typeof setTimeout> | undefined;

    const topAbsoluteRow = (): number => {
      const scrollback = terminalInternals(term).getScrollbackLength?.() ?? 0;
      return scrollback - Math.floor(term?.getViewportY() ?? 0);
    };

    const selectionCellAt = (clientX: number, clientY: number): CellPoint | undefined => {
      const canvas = container?.querySelector<HTMLElement>("canvas:not([aria-hidden='true'])");
      if (!canvas || !term) return undefined;
      const rect = canvas.getBoundingClientRect();
      return cellAtPoint(
        clientX - rect.left,
        clientY - rect.top,
        rect.width,
        rect.height,
        term.cols,
        term.rows,
      );
    };

    // A bare long-press (no drag) selects the word under the finger, like
    // iOS's native text long-press; a space or unreadable line selects just
    // the touched cell so the gesture still gives visible feedback.
    const wordRangeAt = (cell: CellPoint): { start: CellPoint; end: CellPoint } => {
      const line = term?.buffer.active.getLine?.(topAbsoluteRow() + cell.row);
      const text = line?.translateToString?.(false);
      if (!text || !text[cell.col] || text[cell.col] === " ") {
        return { start: cell, end: cell };
      }
      let start = cell.col;
      let end = cell.col;
      while (start > 0 && text[start - 1] !== " ") start -= 1;
      while (end < text.length - 1 && text[end + 1] !== " ") end += 1;
      return { start: { col: start, row: cell.row }, end: { col: end, row: cell.row } };
    };

    const applySelection = (start: CellPoint, end: CellPoint) => {
      const manager = terminalInternals(term).selectionManager;
      if (!manager) return;
      const top = topAbsoluteRow();
      manager.selectionStart = { col: start.col, absoluteRow: top + start.row };
      manager.selectionEnd = { col: end.col, absoluteRow: top + end.row };
      manager.requestRender?.();
    };

    const flashCopyNotice = (message: string) => {
      pasteNotice = message;
      if (copyNoticeTimer) clearTimeout(copyNoticeTimer);
      copyNoticeTimer = setTimeout(() => {
        pasteNotice = "";
        copyNoticeTimer = undefined;
      }, 2500);
    };

    const dismissCopyUi = () => {
      copyOverlayOpen = false;
      copyFallbackOpen = false;
      copyOverlayText = "";
    };

    const finishSelection = (cancelled: boolean) => {
      selectionAnchor = undefined;
      if (cancelled) {
        dismissCopyUi();
        term?.clearSelection();
        return;
      }
      const text = term?.getSelection() ?? "";
      if (!text) {
        dismissCopyUi();
        term?.clearSelection();
        return;
      }
      copyOverlayText = text;
      copyOverlayOpen = true;
      copyFallbackOpen = false;
    };

    handleCopyOverlay = async () => {
      const text = copyOverlayText || term?.getSelection() || "";
      copyOverlayOpen = false;
      const ok = text ? await copyText(text) : false;
      if (ok) {
        flashCopyNotice("Copied");
        copyOverlayText = "";
        copyFallbackOpen = false;
        term?.clearSelection();
        return;
      }
      copyFallbackOpen = true;
    };
    clearTermSelection = () => term?.clearSelection();

    // Touch/wheel scroll, horizontal pan, pinch-zoom, long-press copy, and
    // momentum flings all live in terminalGestures; the component only
    // supplies the terminal-side effects each gesture drives.
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
          pinchEnded: () => {
            pinchFlushPending = true;
            schedulePostLayoutRefit();
            // postLayout runs fit+resize this frame and once more next frame;
            // clear the exemption after that second frame (rAF FIFO ordering
            // guarantees the second refit runs before this clear).
            requestAnimationFrame(() => {
              requestAnimationFrame(() => {
                pinchFlushPending = false;
              });
            });
          },
          beginSelection: (clientX, clientY) => {
            dismissCopyUi();
            selectionAnchor = selectionCellAt(clientX, clientY);
            if (!selectionAnchor) return;
            const word = wordRangeAt(selectionAnchor);
            applySelection(word.start, word.end);
          },
          extendSelection: (clientX, clientY) => {
            if (!selectionAnchor) return;
            const focus = selectionCellAt(clientX, clientY);
            if (!focus) return;
            const { start, end } = orderedSelection(selectionAnchor, focus);
            applySelection(start, end);
          },
          endSelection: finishSelection,
          touchBegan: () => {
            term?.textarea?.focus({ preventScroll: true });
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
      // A pinch-end flush is exempt — it refits the grid and resizes the PTY
      // in the same pass, so lockstep holds.
      if (isKeyboardOpen() && !pinchFlushPending && !expandFlushPending) return;
      if (!term) return;
      const size = validTerminalSize(term.cols, term.rows);
      if (!size) return;
      resizeDedupe.sendIfChanged(size.cols, size.rows);
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
      snapScrollbackToBottom();
      hasUnseenOutput = false;
    };
    // iOS long-press paste doesn't reliably reach the hidden terminal input, so
    // the key bar offers an explicit Paste. term.paste() honors bracketed-paste
    // mode and flows through the normal onData → socket path. Failures must be
    // visible: silently doing nothing reads as a broken button.
    pasteToTerm = (text: string) => {
      term?.paste(text);
      term?.focus();
    };
    requestPaste = () => {
      const clipboard = navigator.clipboard;
      if (!clipboard || typeof clipboard.readText !== "function") {
        // No async clipboard on insecure (plain-http LAN) origins: offer a
        // real textarea the iOS long-press Paste menu can act on instead.
        openPasteFallback();
        return;
      }
      clipboard
        .readText()
        .then((text) => {
          if (text) pasteToTerm(text);
          else term?.focus();
          pasteNotice = "";
        })
        .catch(() => {
          openPasteFallback();
        });
    };

    // Fit rows to the container. Fit mode sizes the PTY to the visible width
    // with a 40-column safety floor so phones get a readable grid without
    // horizontal panning. When the floor exceeds what fits, the font shrinks to
    // keep every column on screen; only sub-minimum overflow can pan.
    let keyboardWasOpen = false;
    let pinchFlushPending = false;
    // The ⛶ expand toggle changes the panel from its padded inline width to the
    // full-bleed fixed overlay. Entering expand focuses the terminal, which pops
    // the iOS keyboard — so fitNow's keyboard-open guard would otherwise skip the
    // grid resize and leave the canvas at its narrower pre-expand column count,
    // left-aligned with an empty column down the right edge. This one-shot flag
    // exempts the expand refit exactly like pinchFlushPending, so the grid (and
    // PTY) resize once to the new width.
    let expandFlushPending = false;
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
      if (keyboardOpen && !pinchFlushPending && !expandFlushPending) {
        // The server resize is withheld while the keyboard is open, so the
        // local grid must not change either: a grid smaller than the PTY makes
        // tmux cursor-address rows that no longer exist locally, and the renderer
        // clamps those writes to its bottom row — the TUI input box drifts up
        // and overwrites the line below it. Keep grid == PTY and crop the
        // taller canvas bottom-anchored so the cursor/input row stays visible
        // above the keyboard. A pinch-end flush is exempt — it refits the grid
        // and resizes the PTY in the same pass, so lockstep holds.
        if (container) {
          container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
        }
        if (pinnedToBottom) snapScrollbackToBottom();
        return;
      }
      if (!term || !fitAddon) return;
      if (container) container.scrollTop = 0;
      const proposed = fitAddon.proposeDimensions();
      // Fit-to-width: the font tracks the operator's chosen size but shrinks
      // as far as the readable minimum so the column floor fits the host —
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
        term.resize(flooredCols(hostFitCols() ?? proposed.cols, colsFloor()), proposed.rows);
      } else {
        // jsdom / pre-layout paints propose nothing; plain fit is the best guess.
        fitAddon.fit();
      }
      clampHorizontalPan();
      if (pinnedToBottom) snapScrollbackToBottom();
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
    // setExpanded has already switched the panel to the full-bleed fixed
    // overlay. Let the post-layout fit run through the keyboard-open guard once,
    // mirroring the pinch-end exemption so the local grid and PTY resize in the
    // same pass.
    beginExpandFlush = () => {
      expandFlushPending = true;
      schedulePostLayoutRefit();
      // snapExpandedView's final settle is setTimeout(260). Keep the exemption
      // through that window so late post-layout refits still resize the grid/PTY
      // while the keyboard is open; clear on the next frame after settle.
      if (expandFlushTimer) clearTimeout(expandFlushTimer);
      const EXPAND_FLUSH_MS = 280;
      expandFlushTimer = setTimeout(() => {
        expandFlushTimer = undefined;
        if (disposed) {
          expandFlushPending = false;
          return;
        }
        schedulePostLayoutRefit();
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            expandFlushPending = false;
          });
        });
      }, EXPAND_FLUSH_MS);
    };

    const snapVisibleTerminal = () => {
      pinnedToBottom = true;
      hasUnseenOutput = false;
      resetDocumentScroll();
      if (isKeyboardOpen() && container) {
        container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
        snapScrollbackToBottom();
      }
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
        schedulePostLayoutRefit();
        const secondFrame = requestAnimationFrame(() => {
          snapFrames = snapFrames.filter((frame) => frame !== secondFrame);
          if (!disposed) {
            snapVisibleTerminal();
            schedulePostLayoutRefit();
          }
        });
        snapFrames.push(secondFrame);
      });
      snapFrames.push(firstFrame);
      snapTimer = setTimeout(() => {
        snapTimer = undefined;
        if (!disposed) {
          snapVisibleTerminal();
          schedulePostLayoutRefit();
        }
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

    // Coalesce WS output chunks into one write + one follow/compensation pass
    // per ~16ms (or max-char) flush. Input / zero-lag echo stays unbatched.
    const writeBatcher = createTerminalWriteBatcher({
      onFlush: (combined) => {
        if (pinnedToBottom) {
          writeToTerminal(combined);
        } else {
          // viewportY is measured from the bottom, so when this write pushes
          // lines into scrollback the view must step back by the same amount
          // or the text being read crawls upward.
          const scrollbackBefore = scrollbackLines();
          writeToTerminal(combined);
          const compensation = scrollbackGrowthCompensation(
            scrollbackBefore,
            scrollbackLines(),
          );
          if (compensation !== 0) term?.scrollLines(compensation);
        }
        const pendingOptimistic = zeroLagInput;
        if (pendingOptimistic && combined.includes(pendingOptimistic)) {
          optimisticPrintableAhead = "";
          optimisticBackspacesAhead = 0;
          zeroLagInput = "";
          zeroLagStyle = "";
        }
        const follow = outputFollowEffects(pinnedToBottom);
        if (follow.snapToBottom) {
          snapScrollbackToBottom();
        } else if (follow.markUnseenOutput) {
          hasUnseenOutput = true;
        }
      },
    });

    connection = connectTaskTerminal(handle, {
      onOutput: (text) => writeBatcher.push(text),
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
        resizeDedupe.reset();
        schedulePostLayoutRefit();
        requestAnimationFrame(() => term?.focus());
      },
    });

    // Exposed to the status banner's manual "Reconnect" button.
    requestReconnect = () => connection.reconnectNow();

    const cursorOverlayStyle = (): string => {
      const canvas = container?.querySelector<HTMLElement>("canvas:not([aria-hidden='true'])");
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
      const fontSize = term.options.fontSize ?? DEFAULT_FONT_SIZE;
      return `left: ${left}px; top: ${top}px; font-size: ${fontSize}px; line-height: ${cellHeight}px;`;
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
        // keydown is not preventDefaulted (so iOS can key-repeat); reseed so
        // the next hold tick still has deletable content.
        if (term?.textarea) seedBackspaceSentinel(term.textarea);
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
      if (pinnedToBottom) snapScrollbackToBottom();
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
        scrollback: terminalScrollbackLines(),
        scrollbarWidth: 0,
        // Ajax synthesizes 100% of scrolling and reads getViewportY() as an
        // instant integer (pinnedToBottom, Math.floor topAbsoluteRow). 0.9.x's
        // smooth scroll animates viewportY to fractional values across frames on
        // the library's own wheel/scrollbar paths, so those reads misfire mid-
        // animation. 0 = instant jump (terminal.ts duration===0 short-circuit),
        // restoring 0.4.0's semantics.
        smoothScrollDuration: 0,
        theme: {
          background: "#1c1714",
          foreground: "#f4eee0",
          cursor: "#52a095",
        },
      });
      term.loadAddon(fitAddon);
      term.open(container);
      // Keep the real scrollToBottom for Ajax's intentional snaps, then blind
      // the instance method so ghostty-web's write-time force-scroll (and the
      // pinnedToBottom-corrupting scroll event it fires) can never run. See
      // the libraryScrollToBottom note above.
      libraryScrollToBottom = term.scrollToBottom.bind(term);
      terminalInternals(term).scrollToBottom = () => {};
      hardenMobileTextarea();
      // iOS hold-to-delete repeats via beforeinput deleteContentBackward.
      // Ghostty's keydown always preventDefault()s Backspace, which cancels that
      // loop. Returning false skips Ghostty's keydown handling without
      // preventDefault; beforeinput still emits \x7f.
      term.attachCustomKeyEventHandler((event) => {
        if (event.key === "Backspace" || event.key === "Delete") return false;
        return undefined;
      });
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
      if (expandFlushTimer) {
        clearTimeout(expandFlushTimer);
        expandFlushTimer = undefined;
      }
      expandFlushPending = false;
      // Paint any queued output while the terminal still exists, then drop the
      // batcher so a late timer cannot write after dispose.
      if (term) writeBatcher.flush();
      writeBatcher.dispose();
      refitScheduler.dispose();
      if (ctrlTimer) clearTimeout(ctrlTimer);
      if (copyNoticeTimer) clearTimeout(copyNoticeTimer);
      connection.dispose();
      for (const subscription of terminalSubscriptions) subscription.dispose();
      term?.textarea?.removeEventListener("beforeinput", handleTextareaBeforeInput);
      term?.textarea?.removeEventListener("focus", handleTextareaFocusForBackspaceSentinel);
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

<div class="terminal-root">
  {#if expanded && inlineSpacerHeight > 0}
    <div
      class="terminal-inline-spacer"
      style="height: {inlineSpacerHeight}px"
      aria-hidden="true"></div>
  {/if}
  <section
    class="terminal-panel"
    class:is-expanded={expanded}
    data-testid="task-terminal-panel"
    data-terminal-engine={placeholderMode ? "placeholder" : "ghostty"}
    aria-label="Task terminal">
  <div class="terminal-host task-terminal-viewport" bind:this={container}>
    {#if placeholderMode}
      <div data-testid="terminal-placeholder" class="terminal-placeholder">Terminal placeholder</div>
    {:else if zeroLagInput}
      <div
        class="terminal-zero-lag-input"
        data-testid="terminal-zero-lag-input"
        aria-hidden="true"
        style={zeroLagStyle}>
        {zeroLagInput}
      </div>
    {/if}
    {#if hasUnseenOutput}
      <button
        type="button"
        class="terminal-new-output"
        onclick={() => {
          jumpToBottom();
        }}>New output ↓</button>
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
        beginExpandFlush();
        snapExpandedView();
      } else {
        blurTerm();
        refitAfterLayout();
      }
    }}>⛶</button>
  {#if copyOverlayOpen}
    <button
      type="button"
      class="terminal-copy-overlay"
      data-testid="terminal-copy-overlay"
      onclick={() => void handleCopyOverlay()}>Copy</button>
  {/if}
  <div
    class="terminal-bottom-controls"
    data-testid="terminal-bottom-controls"
    aria-label="Terminal input controls">
    {#if copyFallbackOpen}
      <div class="terminal-paste-fallback" data-testid="terminal-copy-fallback">
        <textarea
          class="terminal-paste-fallback-input"
          readonly
          rows="3"
          aria-label="Selected text to copy"
          bind:this={copyFallbackInput}
          value={copyOverlayText}></textarea>
        <button
          type="button"
          class="terminal-key"
          onclick={() => {
            copyFallbackOpen = false;
            copyOverlayText = "";
            clearTermSelection();
          }}>Done</button>
      </div>
    {/if}
    {#if pasteFallbackOpen}
      <div class="terminal-paste-fallback" data-testid="terminal-paste-fallback">
        <textarea
          class="terminal-paste-fallback-input"
          placeholder="Long-press here, then tap Paste"
          aria-label="Paste text for the terminal"
          rows="1"
          bind:this={pasteFallbackInput}
          onpaste={(event) => {
            const text = event.clipboardData?.getData("text") ?? "";
            event.preventDefault();
            sendPasteFallbackText(text);
          }}></textarea>
        <button
          type="button"
          class="terminal-key"
          onclick={() => {
            sendPasteFallbackText(pasteFallbackInput?.value ?? "");
          }}>Send</button>
        <button
          type="button"
          class="terminal-key"
          onclick={() => {
            closePasteFallback();
          }}>Cancel</button>
      </div>
    {/if}
    <div
      class="terminal-status"
      class:is-empty={!(status !== "connected" || statusDetail || pasteNotice)}
      data-testid="terminal-status"
      aria-hidden={!(status !== "connected" || statusDetail || pasteNotice)}>
      {#if status !== "connected" || statusDetail || pasteNotice}
        <span class="terminal-status-label">{STATUS_LABELS[status]}</span>
        {#if statusDetail}
          <span class="terminal-status-detail">{statusDetail}</span>
        {/if}
        {#if pasteNotice}
          <span class="terminal-status-detail">{pasteNotice}</span>
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
        onclick={() => requestPaste()}>Paste</button>
      <button
        type="button"
        class="terminal-key"
        aria-label="Hide keyboard"
        onclick={() => blurTerm()}>⌄</button>
    </div>
  </div>
  </section>
</div>

<style>
  .terminal-root {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .terminal-inline-spacer {
    flex: none;
    width: 100%;
    pointer-events: none;
  }

  .terminal-placeholder {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    justify-content: center;
    min-height: 200px;
    color: var(--ink-muted);
    font-size: 12px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

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

  .terminal-copy-overlay {
    position: absolute;
    top: 6px;
    right: 48px;
    z-index: 2;
    min-height: 36px;
    padding: 6px 12px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--paper) 88%, transparent);
    color: var(--ink);
    font-size: 13px;
    font-weight: 700;
  }

  .terminal-expand-corner:hover,
  .terminal-expand-corner:focus-visible {
    border-color: var(--ink-soft);
    color: var(--ink);
    outline: none;
  }

  /* Expanded = the fullscreen fixed panel sits at the screen top, so the
     absolutely-positioned corner (which ignores the panel's safe-area padding)
     would land under the notch/status bar. Offset it by the safe-area insets
     while expanded; both are 0 off-notch and on desktop, so 6px holds there. */
  .terminal-expand-corner.is-armed {
    top: calc(6px + env(safe-area-inset-top));
    right: calc(6px + env(safe-area-inset-right));
    background: var(--teal-deep);
    border-color: var(--teal);
    color: var(--paper);
  }

  .terminal-panel.is-expanded .terminal-copy-overlay {
    top: calc(6px + env(safe-area-inset-top));
    right: calc(48px + env(safe-area-inset-right));
  }

  /* A landscape phone exceeds the width breakpoint but must not get the
     fixed desktop panel height — its takeover layout flex-fills instead. */
  @media (min-width: 768px) and (not ((pointer: coarse) and (max-height: 500px))) {
    .terminal-panel:not(.is-expanded) {
      height: min(58vh, 560px);
      max-height: min(58vh, 560px);
    }
  }

  @media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {
    /* Full-bleed: the task page drops its horizontal padding on mobile so the
       terminal runs edge to edge; side/bottom borders and radii would read as
       stray hairlines against the screen edges. */
    .terminal-panel {
      margin-top: 0;
      border-left: none;
      border-right: none;
      border-bottom: none;
      border-radius: 0;
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
    /* The column floor can make the Ghostty canvas wider than the host.
       The host clips it and the touch handler pans it via
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
    /* ghostty-web marks the host contenteditable for key handling; without
       these, an iOS long-press raises the text-magnifier loupe (a huge
       duplicated echo of the canvas) and the system edit callout, both of
       which fight the synthesized long-press copy gesture. */
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }

  /* The hidden input must stay selectable or iOS refuses to paste into it.
     Soften ghostty's full clip (opacity:0 + clipPath:inset(50%)) just enough
     that iOS treats it as a real edit target for native Paste. */
  .terminal-host :global(textarea) {
    user-select: text;
    -webkit-user-select: text;
    opacity: 0.01;
    clip-path: none;
    -webkit-clip-path: none;
    color: transparent;
    -webkit-text-fill-color: transparent;
    caret-color: transparent;
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

  .terminal-paste-fallback {
    display: flex;
    align-items: stretch;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: var(--paper);
  }

  .terminal-paste-fallback-input {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 44px;
    padding: 8px;
    border: 1px solid var(--rule);
    border-radius: var(--radius-sm);
    background: transparent;
    resize: none;
    font-family: var(--mono);
    /* >= 16px so iOS Safari does not zoom on focus. */
    font-size: 16px;
  }

  .terminal-new-output {
    position: absolute;
    left: 50%;
    bottom: 8px;
    z-index: 2;
    transform: translateX(-50%);
    min-height: 36px;
    margin: 0;
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
    min-height: 28px;
    padding: 0 4px;
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .terminal-status.is-empty {
    visibility: hidden;
    pointer-events: none;
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
