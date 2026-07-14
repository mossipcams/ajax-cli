<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal, FitAddon, type IDisposable } from "ghostty-web";
  import { preloadGhosttyRuntime } from "../terminalPreload";
  import {
    connectTaskTerminal,
    type TerminalConnection,
    type TerminalConnectionStatus,
  } from "../terminalConnection";
  import { isKeyboardOpen, resetDocumentScroll } from "../viewport";
  import { attachTerminalGestures, cellAtPoint, orderedSelection, type CellPoint } from "../terminalGestures";
  import { createRefitScheduler } from "../terminalRefit";
  import { copyText } from "../diagnostics";
  import {
    logicalCols,
    scaledLogicalRows,
    fitScale,
    clampPan,
    fitCapFontSize,
    persistedFontSize,
    persistFontSize,
    terminalScrollbackLines,
    DEFAULT_FONT_SIZE,
    MAX_FONT_SIZE,
    MIN_TERMINAL_COLS,
  } from "../terminalGeometry";
  import {
    scrollbackGrowthCompensation,
    validTerminalSize,
    createResizeDedupe,
    createScrollFollowPolicy,
    createTerminalWriteBatcher,
  } from "../terminalOutputPolicy";
  import {
    createZeroLagEcho,
    createZeroLagOverlayPainter,
    measureZeroLagFromTerminalHost,
  } from "../terminalZeroLag";
  import {
    createTerminalLayoutPolicy,
    EXPAND_REWRAP_MS,
  } from "../terminalLayoutPolicy";
  import {
    createTerminalClipboardUi,
    type ClipboardUiSnapshot,
  } from "../terminalClipboard";
  const TERMINAL_PLACEHOLDER_KEY = "ajax.debug.terminalPlaceholder";
  const placeholderMode =
    typeof localStorage !== "undefined" &&
    localStorage.getItem(TERMINAL_PLACEHOLDER_KEY) === "true";

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

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  /** Ghostty open() parent; CSS scale applies here, never on `.terminal-host`. */
  let scaleLayer: HTMLDivElement | undefined = $state();
  // A dead socket always auto-recovers (terminalConnection's backoff), so
  // there is no terminal "disconnected" state — only the reconnecting one.
  let status = $state<TerminalConnectionStatus>("connecting");
  let statusDetail = $state("");
  // Clipboard feedback is its own channel: paste outcomes must never clear or
  // overwrite a bridge-reported error in statusDetail.
  let pasteNotice = $state("");
  let pasteFallbackOpen = $state(false);
  let pasteFallbackInput = $state<HTMLTextAreaElement | undefined>();
  let copyOverlayOpen = $state(false);
  let copyOverlayText = $state("");
  let copyFallbackOpen = $state(false);
  let copyFallbackInput = $state<HTMLTextAreaElement | undefined>();
  const syncClipboardUi = (snap: ClipboardUiSnapshot) => {
    pasteFallbackOpen = snap.pasteFallbackOpen;
    copyOverlayOpen = snap.copyOverlayOpen;
    copyFallbackOpen = snap.copyFallbackOpen;
    copyOverlayText = snap.copyOverlayText;
    pasteNotice = snap.notice;
  };
  const clipboardUi = createTerminalClipboardUi({ onChange: syncClipboardUi });
  const openPasteFallback = () => clipboardUi.openPasteFallback();
  const closePasteFallback = () => clipboardUi.closePasteFallback();
  const dismissCopyUi = () => clipboardUi.dismissCopyUi();
  $effect(() => {
    if (pasteFallbackOpen) pasteFallbackInput?.focus();
  });
  let ctrlArmed = $state(false);
  let hasUnseenOutput = $state(false);
  let expanded = $state(false);
  let inlineSpacerHeight = $state(0);
  $effect(() => {
    if (copyFallbackOpen) {
      copyFallbackInput?.focus();
      copyFallbackInput?.select();
    }
  });
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
    const trimmed = clipboardUi.takePasteFallbackText(text);
    if (pasteFallbackInput) pasteFallbackInput.value = "";
    if (trimmed) pasteToTerm(trimmed);
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
  let endExpandFlush: () => void = () => {};
  let setScrollOffsetPx: (px: number) => void = () => {};

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
    const scrollFollow = createScrollFollowPolicy();
    const syncScrollFollowUi = () => {
      hasUnseenOutput = scrollFollow.hasUnseen();
    };
    const terminalSubscriptions: IDisposable[] = [];
    const pendingOutput: string[] = [];

    // Auto-follow new output only while the user is at the bottom of the
    // scrollback. A tmux-attached session redraws constantly (status bar,
    // idle prompt refresh), and unconditionally calling scrollToBottom() on
    // every output frame yanked the view back down the instant a user tried
    // to scroll up — scrolling looked completely broken.
    let snapTimer: ReturnType<typeof setTimeout> | undefined;
    let expandRewrapTimer: ReturnType<typeof setTimeout> | undefined;
    let snapFrames: number[] = [];
    const layoutPolicy = createTerminalLayoutPolicy();

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
      input.style.position = "absolute";
      input.style.bottom = "0";
      input.style.height = "44px";
      input.style.width = "100%";
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
      const base =
        height > 0 && term && term.rows > 0 ? height / term.rows : 18;
      return base * terminalFitScale;
    };

    // The operator's font size — persisted pinch choice or the default. The
    // live font may sit below it when the column floor would overflow the host
    // width, and climbs back toward this choice when the viewport widens again.
    let chosenFontSize = persistedFontSize() ?? DEFAULT_FONT_SIZE;

    // CSS scale applied to term.element after fit; 1 on wide hosts.
    let terminalFitScale = 1;
    let subCellOffsetPx = 0;

    const syncScaleLayerTransform = () => {
      if (!container) return;
      const element = scaleLayer ?? term?.element;
      if (!element) return;
      const parts: string[] = [];
      if (subCellOffsetPx !== 0) {
        parts.push(`translateY(${subCellOffsetPx}px)`);
      }
      const scale = terminalFitScale;
      if (scale < 1) {
        parts.push(`scale(${scale})`);
        element.style.transformOrigin = "0 0";
      } else {
        element.style.transformOrigin = "";
      }
      element.style.transform = parts.length ? parts.join(" ") : "";
    };

    const setScrollOffsetPxImpl = (px: number) => {
      if (px === subCellOffsetPx) return;
      subCellOffsetPx = px;
      syncScaleLayerTransform();
    };
    setScrollOffsetPx = setScrollOffsetPxImpl;

    const colsFloor = () => MIN_TERMINAL_COLS;

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

    const fitFontCap = (): number => {
      const hostFit = hostFitCols();
      const proposed = fitAddon?.proposeDimensions()?.cols;
      const fitProposal = hostFit ?? proposed;
      if (
        fitProposal !== undefined &&
        logicalCols(fitProposal) > Math.floor(fitProposal)
      ) {
        return MAX_FONT_SIZE;
      }
      return (
        hostWidthFitCap() ??
        fitCapFontSize(
          term?.options.fontSize ?? DEFAULT_FONT_SIZE,
          fitAddon?.proposeDimensions()?.cols,
          colsFloor(),
        )
      );
    };

    const applyTerminalScale = () => {
      if (!container) return;
      const element = scaleLayer ?? term?.element;
      if (!element || !term) return;
      const cellWidth = (term as TerminalWithRendererMetrics).renderer?.getMetrics?.()?.width;
      const hostWidth = container.clientWidth;
      const cols = term.cols;
      if (
        !Number.isFinite(hostWidth) ||
        hostWidth <= 0 ||
        !cellWidth ||
        !Number.isFinite(cellWidth) ||
        cellWidth <= 0 ||
        !Number.isFinite(cols) ||
        cols <= 0
      ) {
        terminalFitScale = 1;
        syncScaleLayerTransform();
        return;
      }
      terminalFitScale = fitScale(hostWidth, cols, cellWidth);
      syncScaleLayerTransform();
    };

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

    const topAbsoluteRow = (): number => {
      const scrollback = terminalInternals(term).getScrollbackLength?.() ?? 0;
      return scrollback - Math.floor(term?.getViewportY() ?? 0);
    };

    const selectionCellAt = (clientX: number, clientY: number): CellPoint | undefined => {
      const canvas = container?.querySelector<HTMLElement>("canvas:not([aria-hidden='true'])");
      if (!canvas || !term) return undefined;
      const rect = canvas.getBoundingClientRect();
      const scale = terminalFitScale > 0 ? terminalFitScale : 1;
      return cellAtPoint(
        (clientX - rect.left) / scale,
        (clientY - rect.top) / scale,
        rect.width / scale,
        rect.height / scale,
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
      clipboardUi.presentCopySelection(text);
    };

    handleCopyOverlay = async () => {
      const text = clipboardUi.beginCopyAttempt() || term?.getSelection() || "";
      const ok = text ? await copyText(text) : false;
      if (ok) {
        clipboardUi.noteCopySucceeded();
        term?.clearSelection();
        return;
      }
      clipboardUi.noteCopyFailed();
    };
    clearTermSelection = () => term?.clearSelection();

    // Touch/wheel scroll, horizontal pan, pinch-zoom, long-press copy, and
    // momentum flings all live in terminalGestures; the component only
    // supplies the terminal-side effects each gesture drives.
    const detachGestures = container
      ? attachTerminalGestures(container, {
          scrollLines: (lines) => {
            if (lines !== 0) {
              scrollFollow.unpin();
              syncScrollFollowUi();
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
            layoutPolicy.pinchEnded();
            schedulePostLayoutRefit();
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
            resetDocumentScroll();
            scrollFollow.pin();
            syncScrollFollowUi();
            if (container) {
              container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
            }
            if (isKeyboardOpen() && scrollFollow.isPinned()) snapScrollbackToBottom();
            term?.textarea?.focus({ preventScroll: true });
          },
          setScrollOffsetPx: setScrollOffsetPxImpl,
          atBottom: () => (term?.getViewportY() ?? 0) <= 0,
          atTop: () => (term ? term.getViewportY() >= scrollbackLines() : true),
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
      const decision = layoutPolicy.setKeyboardOpen(isKeyboardOpen());
      if (!decision.allowPtyResize) return;
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
      setScrollOffsetPxImpl(0);
      const follow = scrollFollow.jumpToBottom();
      if (follow.snapToBottom) snapScrollbackToBottom();
      syncScrollFollowUi();
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
          clipboardUi.clearNotice();
        })
        .catch(() => {
          openPasteFallback();
        });
    };

    // Agent-sized fit: logical cols stay at least 80; CSS scale shrinks the
    // canvas to the host width on phones so live and scrollback share layout.
    const clampHorizontalPan = () => {
      if (!container) return;
      container.scrollLeft = clampPan(container.scrollLeft, container.scrollWidth, container.clientWidth);
    };
    const fitNow = () => {
      setScrollOffsetPxImpl(0);
      const decision = layoutPolicy.setKeyboardOpen(isKeyboardOpen());
      if (decision.pinToBottomOnKeyboardOpen) {
        scrollFollow.pin();
        syncScrollFollowUi();
      }
      if (!decision.allowLocalFit) {
        if (decision.cropToBottom && container) {
          container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
        }
        if (scrollFollow.isPinned()) snapScrollbackToBottom();
        return;
      }
      if (!term || !fitAddon) return;
      if (container) container.scrollTop = 0;
      const proposed = fitAddon.proposeDimensions();
      const currentFont = term.options.fontSize ?? DEFAULT_FONT_SIZE;
      const fitProposal = hostFitCols() ?? proposed?.cols;
      const usingScale =
        fitProposal !== undefined &&
        logicalCols(fitProposal) > Math.floor(fitProposal);
      if (!usingScale) {
        const cap =
          hostWidthFitCap() ?? fitCapFontSize(currentFont, proposed?.cols, colsFloor());
        const growCeiling = cap >= MAX_FONT_SIZE ? cap : cap - 1;
        const grownFont = Math.min(chosenFontSize, growCeiling, currentFont + 1);
        const nextFont = currentFont > cap ? cap : Math.max(currentFont, grownFont);
        if (nextFont !== currentFont) {
          term.options.fontSize = nextFont;
          scheduleDebouncedRefit();
        }
      } else {
        const grownFont = Math.min(chosenFontSize, currentFont + 1, MAX_FONT_SIZE);
        const nextFont = Math.max(currentFont, grownFont);
        if (nextFont !== currentFont) {
          term.options.fontSize = nextFont;
          scheduleDebouncedRefit();
        }
      }
      if (proposed && Number.isFinite(proposed.rows) && proposed.rows > 0) {
        const cols = logicalCols(fitProposal);
        const cellWidth = (term as TerminalWithRendererMetrics).renderer?.getMetrics?.()?.width;
        const hostWidth = container?.clientWidth;
        const scale =
          cellWidth &&
          Number.isFinite(cellWidth) &&
          cellWidth > 0 &&
          hostWidth !== undefined &&
          Number.isFinite(hostWidth) &&
          hostWidth > 0
            ? fitScale(hostWidth, cols, cellWidth)
            : 1;
        const rows = scaledLogicalRows(proposed.rows, scale);
        term.resize(cols, rows);
        applyTerminalScale();
      } else {
        fitAddon.fit();
        applyTerminalScale();
      }
      clampHorizontalPan();
      if (scrollFollow.isPinned()) snapScrollbackToBottom();
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
      layoutPolicy.expandEnter();
      schedulePostLayoutRefit();
      // snapExpandedView's final settle is setTimeout(260). Schedule one more
      // post-layout refit after the expand rewrap window so late layout reads
      // still resize the grid/PTY while the keyboard is open.
      if (expandRewrapTimer) clearTimeout(expandRewrapTimer);
      expandRewrapTimer = setTimeout(() => {
        expandRewrapTimer = undefined;
        if (!disposed) schedulePostLayoutRefit();
      }, EXPAND_REWRAP_MS);
    };
    endExpandFlush = () => {
      layoutPolicy.expandExit();
    };

    const snapVisibleTerminal = () => {
      scrollFollow.pin();
      syncScrollFollowUi();
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

    const zeroLagPainter = createZeroLagOverlayPainter(() => container);
    const zeroLag = createZeroLagEcho({
      onChange: (text, style) => zeroLagPainter.paint(text, style),
      measure: () => {
        const measured = measureZeroLagFromTerminalHost({
          host: container,
          term,
          defaultFontSize: DEFAULT_FONT_SIZE,
        });
        if (!measured || terminalFitScale >= 1) return measured;
        return { ...measured, fitScale: terminalFitScale };
      },
    });

    // Coalesce WS output chunks into one write + one follow/compensation pass
    // per ~16ms (or max-char) flush. Input / zero-lag echo stays unbatched.
    const writeBatcher = createTerminalWriteBatcher({
      onFlush: (combined) => {
        if (scrollFollow.isPinned()) {
          setScrollOffsetPxImpl(0);
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
        zeroLag.clearIfEchoedIn(combined);
        const follow = scrollFollow.noteOutput();
        if (follow.snapToBottom) {
          snapScrollbackToBottom();
        } else if (follow.markUnseenOutput) {
          syncScrollFollowUi();
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
      onOpen: (isReconnect) => {
        setScrollOffsetPxImpl(0);
        statusDetail = "";
        zeroLag.reset();
        resizeDedupe.reset();
        if (isReconnect) {
          pendingOutput.length = 0;
          term?.reset();
          scrollFollow.resetOnReconnect();
          syncScrollFollowUi();
          snapScrollbackToBottom();
        }
        schedulePostLayoutRefit();
        requestAnimationFrame(() => term?.focus());
      },
    });

    // Exposed to the status banner's manual "Reconnect" button.
    requestReconnect = () => connection.reconnectNow();

    const handleTextareaBeforeInput = (event: InputEvent) => {
      if (event.inputType === "insertText" && event.data) {
        zeroLag.noteBeforeInputPrintable(event.data);
        return;
      }
      if (event.inputType === "deleteContentBackward") {
        zeroLag.noteBeforeInputBackspace();
        // keydown is not preventDefaulted (so iOS can key-repeat); reseed so
        // the next hold tick still has deletable content.
        if (term?.textarea) seedBackspaceSentinel(term.textarea);
        return;
      }
      if (event.inputType === "insertLineBreak") {
        zeroLag.clear();
      }
    };

    const handleTerminalData = (raw: string) => {
      if (!connection.isOpen()) return;

      // Sticky Ctrl folds into this key (letter → control code, cursor key →
      // Ctrl-modified CSI). The folded byte then takes the normal branches, so
      // keys Ctrl leaves untouched (Enter, backspace) keep their overlay
      // bookkeeping instead of slipping past it.
      const data = consumeCtrl(raw);

      if (data === "\r") {
        zeroLag.onTerminalData(data);
        connection.sendInput(data);
        return;
      }

      if (data === "\x7f") {
        zeroLag.onTerminalData(data);
        connection.sendInput(data);
        return;
      }

      if (data.length === 1 && data.charCodeAt(0) >= 32) {
        zeroLag.onTerminalData(data);
      }

      connection.sendInput(data);
    };

    const flushPendingOutput = () => {
      if (!term) return;
      for (const text of pendingOutput) term.write(text);
      pendingOutput.length = 0;
      if (scrollFollow.isPinned()) snapScrollbackToBottom();
    };

    const mountGhosttyTerminal = async () => {
      const ghostty = await preloadGhosttyRuntime();
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
      // ghostty-web sets `this.element = parent` in open(). Scaling the host
      // crushed the whole viewport into the top-left corner and broke expand.
      // Open into an inner layer so transform never touches `.terminal-host`.
      if (!scaleLayer) return;
      term.open(scaleLayer);
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
          scrollFollow.setPinnedFromViewport(term ? term.getViewportY() <= 0 : true);
          syncScrollFollowUi();
        }),
        term.onData(handleTerminalData),
      );
      // E2E-only probe: fixtures set window.__ajaxTerminalProbeEnable before boot
      // so Playwright can assert buffer integrity without canvas OCR.
      const probeHost = window as Window & {
        __ajaxTerminalProbeEnable?: boolean;
        __ajaxTerminalProbe?: {
          cols(): number;
          rows(): number;
          viewportY(): number;
          lines(): string[];
        };
      };
      if (probeHost.__ajaxTerminalProbeEnable) {
        probeHost.__ajaxTerminalProbe = {
          cols: () => term?.cols ?? 0,
          rows: () => term?.rows ?? 0,
          viewportY: () => term?.getViewportY() ?? 0,
          lines: () => {
            if (!term) return [];
            const buf = term.buffer.active;
            const out: string[] = [];
            for (let i = 0; i < buf.length; i += 1) {
              const line = buf.getLine?.(i)?.translateToString?.(true) ?? "";
              out.push(line.replace(/\s+$/u, ""));
            }
            return out;
          },
        };
      }
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
      layoutPolicy.dispose();
      if (expandRewrapTimer) {
        clearTimeout(expandRewrapTimer);
        expandRewrapTimer = undefined;
      }
      // Paint any queued output while the terminal still exists, then drop the
      // batcher so a late timer cannot write after dispose.
      if (term) writeBatcher.flush();
      writeBatcher.dispose();
      zeroLag.reset();
      zeroLagPainter.dispose();
      refitScheduler.dispose();
      if (ctrlTimer) clearTimeout(ctrlTimer);
      clipboardUi.dispose();
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
      const probeHost = window as Window & { __ajaxTerminalProbe?: unknown };
      if (probeHost.__ajaxTerminalProbe) delete probeHost.__ajaxTerminalProbe;
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
    <div class="terminal-scale-layer" bind:this={scaleLayer}></div>
    {#if placeholderMode}
      <div data-testid="terminal-placeholder" class="terminal-placeholder">Terminal placeholder</div>
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
      setScrollOffsetPx(0);
      setExpanded(next);
      if (next) {
        focusTerm();
        beginExpandFlush();
        snapExpandedView();
      } else {
        endExpandFlush();
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
            dismissCopyUi();
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
    /* Sub-cell drag offset can expose up to one cell of host; match the
       terminal canvas background so the strip reads as padding, not a gap. */
    background: #1c1714;
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

  /* Ghostty open() target. Scale-to-fit transforms this node only — never the
     host — so the expand control and host hit box stay full-size. */
  .terminal-scale-layer {
    position: absolute;
    left: 0;
    top: 0;
    width: 100%;
    height: 100%;
    transform-origin: 0 0;
  }

  /* The hidden input must stay selectable or iOS refuses to paste into it.
     Soften ghostty's full clip (opacity:0 + clipPath:inset(50%)) just enough
     that iOS treats it as a real edit target for native Paste. */
  .terminal-host :global(textarea) {
    position: absolute;
    bottom: 0;
    height: 44px;
    width: 100%;
    user-select: text;
    -webkit-user-select: text;
    opacity: 0.01;
    clip-path: none;
    -webkit-clip-path: none;
    color: transparent;
    -webkit-text-fill-color: transparent;
    caret-color: transparent;
  }

  /* Imperative overlay (createZeroLagEcho → createZeroLagOverlayPainter). Position comes only
     from zeroLagOverlayStyle (left/top) — never CSS bottom, or top+bottom
     stretches into a second full-height echo. :global so the JS-created node
     still gets these rules under Svelte scoping. */
  .terminal-host :global(.terminal-zero-lag-input) {
    position: absolute;
    z-index: 1;
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
    /* Overlay-only scroll policy: the key row pans but never paints a bar. */
    -ms-overflow-style: none;
    scrollbar-width: none;
    padding: 2px 4px;
    background: var(--paper);
  }

  .terminal-keys::-webkit-scrollbar {
    display: none;
    width: 0;
    height: 0;
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

  :global(html.keyboard-open) .terminal-bottom-controls {
    /* Keyboard covers the home indicator; safe-area pad is dead space. */
    padding-bottom: 6px;
  }

  :global(html.keyboard-open) .terminal-status.is-empty {
    display: none;
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
