import { useState, useEffect, useEffectEvent, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { copyText } from "@/shared/lib/clipboard";
import { resetDocumentScroll } from "@/shared/lib/viewport";
import {
  connectTaskTerminal,
  type TerminalConnection,
  type TerminalConnectionStatus,
} from "@/shared/lib/terminalConnection";
import {
  MIN_TERMINAL_COLS,
  DEFAULT_FONT_SIZE,
  MIN_FONT_SIZE,
  MAX_FONT_SIZE,
  FONT_STORAGE_KEY,
  parsePersistedFontSize,
  computeTerminalGeometry,
} from "@/shared/lib/terminalGeometry";
import { createRefitController } from "@/shared/lib/terminalRefit";
import { createTerminalScrollSync } from "@/shared/lib/terminalScrollSync";

interface Props {
  handle: string;
}

export default function TaskTerminal({ handle }: Props) {
  const hostElRef = useRef<HTMLDivElement | null>(null);
  const interactionElRef = useRef<HTMLDivElement | null>(null);
  const spacerElRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | undefined>(undefined);
  const connectionRef = useRef<TerminalConnection | undefined>(undefined);
  const schedulePostLayoutRef = useRef<((discreteIntent?: boolean) => void) | undefined>(
    undefined,
  );
  const resetResizeDedupeRef = useRef<(() => void) | undefined>(undefined);
  const jumpToBottomRef = useRef<(() => void) | undefined>(undefined);
  const inertedElementsRef = useRef<HTMLElement[]>([]);
  const copyNoticeTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const ctrlTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const expandSettleFrame1Ref = useRef(0);
  const expandSettleFrame2Ref = useRef(0);
  const expandSettleTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pasteFallbackOwnedFocusRef = useRef(false);
  const toolbarPointerOwnedFocusRef = useRef(false);
  const ctrlArmedRef = useRef(false);

  const [status, setStatus] = useState<TerminalConnectionStatus>("connecting");
  const [statusDetail, setStatusDetail] = useState("");
  const [ctrlArmed, setCtrlArmed] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [hasUnseenOutput, setHasUnseenOutput] = useState(false);
  const [pasteFallbackOpen, setPasteFallbackOpen] = useState(false);
  const [pasteFallbackText, setPasteFallbackText] = useState("");
  const [pasteNotice, setPasteNotice] = useState("");
  const [copyOverlayText, setCopyOverlayText] = useState("");
  const [copyNotice, setCopyNotice] = useState("");
  const [copyFallbackOpen, setCopyFallbackOpen] = useState(false);
  const [copyFallbackText, setCopyFallbackText] = useState("");

  const statusVisible = status !== "connected" || statusDetail.length > 0;
  const showReconnect = status === "reconnecting" || status === "unavailable";

  const isPhoneTerminalLayout = () =>
    window.matchMedia("(max-width: 767px), (pointer: coarse) and (max-height: 500px)").matches;

  const clearExpandedInert = () => {
    for (const el of inertedElementsRef.current) {
      el.inert = false;
    }
    inertedElementsRef.current = [];
  };

  const applyExpandedInert = () => {
    clearExpandedInert();
    if (!isPhoneTerminalLayout()) return;

    const panel = hostElRef.current?.closest<HTMLElement>('[data-testid="task-terminal-panel"]');
    const taskDetail = panel?.closest<HTMLElement>(".task-detail");
    const next: HTMLElement[] = [];

    if (taskDetail && panel) {
      for (const child of taskDetail.children) {
        if (child instanceof HTMLElement && !child.contains(panel)) {
          next.push(child);
        }
      }
    }

    for (const el of document.querySelectorAll<HTMLElement>(
      ".cockpit-chrome, .bottom-nav, .result-panel",
    )) {
      next.push(el);
    }

    for (const el of next) {
      if (el.inert) continue;
      el.inert = true;
      inertedElementsRef.current.push(el);
    }
  };

  const syncExpandedInert = (active: boolean) => {
    if (active) applyExpandedInert();
    else clearExpandedInert();
  };

  const EXPAND_REWRAP_MS = 280;
  const EXPANDED_CLASS = "terminal-expanded";
  const PINCH_ACTIVATION_PX = 12;
  const LONG_PRESS_MS = 500;
  const LONG_PRESS_MOVE_CANCEL_PX = 8;
  const DIRECTIONAL_DRAG_THRESHOLD_PX = 24;
  const DIRECTIONAL_REPEAT_INTERVAL_MS = 75;

  const CONTROL_KEYS = [
    { label: "Esc", ariaLabel: "Escape", data: "\x1b" },
    { label: "Tab", ariaLabel: "Tab", data: "\t" },
    { label: "⌃C", ariaLabel: "Control C", data: "\x03" },
    { label: "←", ariaLabel: "Left arrow", data: "\x1b[D" },
    { label: "↑", ariaLabel: "Up arrow", data: "\x1b[A" },
    { label: "↓", ariaLabel: "Down arrow", data: "\x1b[B" },
    { label: "→", ariaLabel: "Right arrow", data: "\x1b[C" },
  ];

  const CTRL_ARM_TIMEOUT_MS = 4000;

  const STATUS_LABELS: Record<TerminalConnectionStatus, string> = {
    connecting: "Connecting…",
    connected: "Connected",
    reconnecting: "Reconnecting…",
    unavailable: "Unavailable",
  };

  const cancelExpandSettle = () => {
    if (expandSettleFrame1Ref.current) {
      cancelAnimationFrame(expandSettleFrame1Ref.current);
      expandSettleFrame1Ref.current = 0;
    }
    if (expandSettleFrame2Ref.current) {
      cancelAnimationFrame(expandSettleFrame2Ref.current);
      expandSettleFrame2Ref.current = 0;
    }
    if (expandSettleTimerRef.current) {
      clearTimeout(expandSettleTimerRef.current);
      expandSettleTimerRef.current = undefined;
    }
  };

  const scheduleBandSettle = () => {
    cancelExpandSettle();
    schedulePostLayoutRef.current?.(true);
    expandSettleFrame1Ref.current = requestAnimationFrame(() => {
      expandSettleFrame1Ref.current = 0;
      schedulePostLayoutRef.current?.(true);
      expandSettleFrame2Ref.current = requestAnimationFrame(() => {
        expandSettleFrame2Ref.current = 0;
        schedulePostLayoutRef.current?.(true);
      });
    });
    expandSettleTimerRef.current = setTimeout(() => {
      expandSettleTimerRef.current = undefined;
      schedulePostLayoutRef.current?.(true);
    }, EXPAND_REWRAP_MS);
  };

  function loadPersistedFontSize(): number {
    try {
      return parsePersistedFontSize(localStorage.getItem(FONT_STORAGE_KEY));
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
    ctrlArmedRef.current = false;
    setCtrlArmed(false);
    if (ctrlTimerRef.current) {
      clearTimeout(ctrlTimerRef.current);
      ctrlTimerRef.current = undefined;
    }
  };

  const toggleCtrl = () => {
    if (ctrlArmedRef.current) {
      disarmCtrl();
      return;
    }
    ctrlArmedRef.current = true;
    setCtrlArmed(true);
    if (ctrlTimerRef.current) clearTimeout(ctrlTimerRef.current);
    ctrlTimerRef.current = setTimeout(disarmCtrl, CTRL_ARM_TIMEOUT_MS);
  };

  const controlModify = (data: string): string => {
    if (data.length === 1) {
      const code = data.toLowerCase().charCodeAt(0);
      if (code >= 97 && code <= 122) return String.fromCharCode(code - 96);
    }
    // ANSI CSI cursor sequences are the point of this match.
    // eslint-disable-next-line no-control-regex -- CSI ESC must appear in the pattern
    const cursor = /^\x1b\[([ABCD])$/.exec(data);
    if (cursor) return `\x1b[1;5${cursor[1]}`;
    return data;
  };

  const consumeCtrl = (data: string): string => {
    if (!ctrlArmedRef.current) return data;
    disarmCtrl();
    return controlModify(data);
  };

  const sendKey = (data: string) => {
    if (!connectionRef.current?.isOpen()) return;
    connectionRef.current.sendInput(data);
  };

  const PASTE_DISCONNECTED_NOTICE = "Terminal disconnected — paste kept below.";

  const pasteToPty = (text: string): boolean => {
    if (!text || !connectionRef.current?.isOpen()) return false;
    const payload = termRef.current?.modes.bracketedPasteMode
      ? `\x1b[200~${text}\x1b[201~`
      : text;
    connectionRef.current.sendInput(payload);
    return true;
  };

  const termTextarea = (): HTMLTextAreaElement | null => {
    const el = termRef.current?.element?.querySelector("textarea.xterm-helper-textarea");
    return el instanceof HTMLTextAreaElement ? el : null;
  };

  const hardenMobileTextarea = () => {
    const input = termTextarea();
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
    input.style.opacity = "0.01";
    input.style.setProperty("clip-path", "none");
    input.style.setProperty("-webkit-clip-path", "none");
    input.style.setProperty("clip", "auto");
    input.style.color = "transparent";
    input.style.setProperty("-webkit-text-fill-color", "transparent");
    input.style.caretColor = "transparent";
  };

  const termOwnedFocus = (): boolean => {
    const textarea = termTextarea();
    return textarea !== null && document.activeElement === textarea;
  };

  const refocusTermIfOwned = (ownedFocus: boolean) => {
    if (!ownedFocus) return;
    const textarea = termTextarea();
    if (textarea) {
      textarea.focus({ preventScroll: true });
      return;
    }
    termRef.current?.focus();
  };

  const blurTerm = () => {
    termTextarea()?.blur();
  };

  const onToolbarPointerDown = (event: React.PointerEvent) => {
    event.preventDefault();
    toolbarPointerOwnedFocusRef.current = termOwnedFocus();
  };

  const consumeToolbarPointerOwnedFocus = (event: React.MouseEvent): boolean => {
    const owned = toolbarPointerOwnedFocusRef.current && event.detail !== 0;
    toolbarPointerOwnedFocusRef.current = false;
    return owned;
  };

  const openPasteFallback = (ownedFocus: boolean, notice: string, text = "") => {
    pasteFallbackOwnedFocusRef.current = ownedFocus;
    setPasteNotice(notice);
    setPasteFallbackText(text);
    setPasteFallbackOpen(true);
  };

  const retainUnsentPaste = (text: string, ownedFocus: boolean) => {
    openPasteFallback(ownedFocus, PASTE_DISCONNECTED_NOTICE, text);
  };

  const dismissPasteFallback = (): boolean => {
    const ownedFocus = pasteFallbackOwnedFocusRef.current;
    setPasteFallbackOpen(false);
    setPasteFallbackText("");
    setPasteNotice("");
    pasteFallbackOwnedFocusRef.current = false;
    return ownedFocus;
  };

  const closePasteFallback = () => {
    refocusTermIfOwned(dismissPasteFallback());
  };

  const pasteThroughTerm = (text: string, ownedFocus = true): boolean => {
    if (!text || !termRef.current) return false;
    if (!pasteToPty(text)) {
      retainUnsentPaste(text, ownedFocus);
      return false;
    }
    refocusTermIfOwned(ownedFocus);
    return true;
  };

  const requestPaste = async (ownedFocus: boolean) => {
    try {
      const readText = navigator.clipboard?.readText;
      if (!readText) {
        openPasteFallback(ownedFocus, "Clipboard unavailable — paste below.");
        return;
      }
      const text = await readText.call(navigator.clipboard);
      if (!text) {
        refocusTermIfOwned(ownedFocus);
        return;
      }
      pasteThroughTerm(text, ownedFocus);
    } catch {
      openPasteFallback(ownedFocus, "Clipboard denied — paste below.");
    }
  };

  const sendPasteFallback = () => {
    const text = pasteFallbackText;
    const ownedFocus = pasteFallbackOwnedFocusRef.current;
    if (!text) {
      closePasteFallback();
      return;
    }
    if (!pasteToPty(text)) {
      setPasteNotice(PASTE_DISCONNECTED_NOTICE);
      return;
    }
    dismissPasteFallback();
    refocusTermIfOwned(ownedFocus);
  };

  const cancelPasteFallback = () => {
    closePasteFallback();
  };

  const syncCopyOverlay = () => {
    const selection = termRef.current?.getSelection() ?? "";
    setCopyOverlayText(selection);
    if (!selection && !copyNoticeTimerRef.current) setCopyNotice("");
  };

  const dismissCopyFallback = () => {
    setCopyFallbackOpen(false);
    setCopyFallbackText("");
  };

  const copySelection = async () => {
    const text = copyOverlayText || termRef.current?.getSelection() || "";
    if (!text) return;
    const copied = await copyText(text);
    if (copied) {
      if (copyNoticeTimerRef.current) clearTimeout(copyNoticeTimerRef.current);
      setCopyNotice("Copied");
      copyNoticeTimerRef.current = setTimeout(() => {
        setCopyNotice("");
        copyNoticeTimerRef.current = undefined;
      }, 1500);
      termRef.current?.clearSelection();
      setCopyOverlayText("");
      return;
    }
    setCopyFallbackText(text);
    setCopyFallbackOpen(true);
  };

  const requestReconnect = () => {
    connectionRef.current?.reconnectNow();
  };

  const toggleExpanded = () => {
    const entering = !expanded;
    setExpanded(entering);
    document.documentElement.classList.toggle(EXPANDED_CLASS, entering);
    syncExpandedInert(entering);
    resetResizeDedupeRef.current?.();
    if (!entering) {
      blurTerm();
      // Exit while keyboard-open used to call discreteIntent=false, which is a
      // no-op under the fit freeze — inline band never refit. Always settle.
      scheduleBandSettle();
      return;
    }
    scheduleBandSettle();
  };

  // ponytail: this useEffect is the entire terminal mount lifecycle (~700
  // lines: socket setup/teardown, gestures, clipboard, fullscreen, geometry,
  // refit, status). The effect events below make its [handle] dep honest
  // rather than suppressed; splitting the body into a disposable controller
  // with [handle, hostElements] deps is round 2's work, not this change.
  const onHardenTextarea = useEffectEvent(() => {
    hardenMobileTextarea();
  });
  const onBandSettle = useEffectEvent(() => {
    scheduleBandSettle();
  });
  const onTermData = useEffectEvent((data: string) => {
    sendKey(consumeCtrl(data));
  });

  useEffect(() => {
    const hostEl = hostElRef.current;
    const interactionEl = interactionElRef.current;
    const spacerEl = spacerElRef.current;
    if (!hostEl || !interactionEl || typeof window.matchMedia !== "function") {
      return;
    }

    // Deferred init: closed over by fitLocal / cleanup before first assignment.
    // eslint-disable-next-line prefer-const -- assigned once after helper closures are built
    let fitAddon: FitAddon | undefined;
    let lastSentCols = 0;
    let lastSentRows = 0;
    let fitFrame = 0;
    let pendingPostKeyboardResync = false;
    let disposed = false;
    let pinchStartDistance = 0;
    let pinchBaseFontSize = DEFAULT_FONT_SIZE;
    let pinchEngaged = false;
    let longPressTimer: ReturnType<typeof setTimeout> | undefined;
    let longPressStartX = 0;
    let longPressStartY = 0;
    let longPressStartedAt = 0;
    let longPressActive = false;
    let longPressSelected = false;
    // ponytail: one-finger held cardinal drag only; ceiling is component-local touch state
    let directionalArmed = false;
    let directionalArrow: string | undefined;
    let directionalRepeatInterval: ReturnType<typeof setInterval> | undefined;

    const isKeyboardOpen = () => document.documentElement.classList.contains("keyboard-open");

    const isActive = () => !disposed;

    const cancelScheduledWork = () => {
      if (fitFrame) {
        cancelAnimationFrame(fitFrame);
        fitFrame = 0;
      }
    };

    // eslint-disable-next-line prefer-const -- assigned once after fitLocal exists
    let refitController: ReturnType<typeof createRefitController> | undefined;

    const resetDedupe = () => {
      lastSentCols = 0;
      lastSentRows = 0;
      refitController?.noteReconnect();
    };
    resetResizeDedupeRef.current = resetDedupe;

    const sendResizeNow = (discreteIntent = false) => {
      if (!isActive() || !connectionRef.current?.isOpen() || !termRef.current) return;
      if (isKeyboardOpen() && !discreteIntent) return;
      const cols = termRef.current.cols;
      const rows = termRef.current.rows;
      if (!Number.isInteger(cols) || !Number.isInteger(rows) || cols <= 0 || rows <= 0) return;
      if (cols === lastSentCols && rows === lastSentRows) return;
      lastSentCols = cols;
      lastSentRows = rows;
      connectionRef.current.sendResize(cols, rows);
    };

    const clearTermScale = (termEl: HTMLElement) => {
      termEl.style.transform = "";
      termEl.style.transformOrigin = "";
      termEl.style.width = "";
      termEl.style.height = "";
    };

    // Pin host height in px only when the wrap's height is flex-indefinite
    // (keyboard-open / fullscreen). Capped inline uses CSS height:100%.
    const syncHostToWrap = () => {
      if (!hostEl || !interactionEl) return;
      const needsPin =
        document.documentElement.classList.contains("keyboard-open") ||
        document.documentElement.classList.contains(EXPANDED_CLASS);
      if (!needsPin) {
        if (hostEl.style.height) hostEl.style.height = "";
        return;
      }
      const next = `${Math.max(0, interactionEl.clientHeight)}px`;
      if (hostEl.style.height !== next) hostEl.style.height = next;
    };

    const fitLocal = () => {
      if (!isActive() || !fitAddon || !termRef.current || !hostEl) return;
      syncHostToWrap();
      const proposed = fitAddon.proposeDimensions();
      if (!proposed) return;

      const termEl = termRef.current.element;
      if (!termEl) return;

      const hostWidth = hostEl.clientWidth;
      const hostHeight = hostEl.clientHeight;
      const currentFontSize = termRef.current.options.fontSize ?? DEFAULT_FONT_SIZE;

      let cellWidth = 1;
      let cellHeight = 1;
      if (proposed.cols < MIN_TERMINAL_COLS) {
        const screenEl = termEl.querySelector<HTMLElement>(".xterm-screen");
        cellWidth =
          screenEl && termRef.current.cols > 0
            ? screenEl.offsetWidth / termRef.current.cols
            : hostWidth / proposed.cols;
        cellHeight =
          screenEl && termRef.current.rows > 0
            ? screenEl.offsetHeight / termRef.current.rows
            : hostHeight / proposed.rows;
      }

      const result = computeTerminalGeometry({
        proposedCols: proposed.cols,
        proposedRows: proposed.rows,
        hostWidth,
        hostHeight,
        cellWidth,
        cellHeight,
        fontSize: currentFontSize,
      });
      if (!result) return;

      if (proposed.cols >= MIN_TERMINAL_COLS) {
        clearTermScale(termEl);
        if (termRef.current.cols !== proposed.cols || termRef.current.rows !== proposed.rows) {
          termRef.current.resize(proposed.cols, proposed.rows);
        }
        return;
      }

      termRef.current.resize(result.cols, result.rows);
      termEl.style.width = `${result.logicalWidth}px`;
      termEl.style.height = `${result.logicalHeight}px`;
      termEl.style.transformOrigin = "0 0";
      termEl.style.transform = `scale(${result.scale})`;
    };

    refitController = createRefitController({
      // Re-check the ambient guards at frame time, not just when the refit
      // was requested: a fit that lands mid-selection resizes the grid,
      // clears the selection, and unmounts the Copy overlay under the tap.
      fit: () => {
        if (isKeyboardOpen()) return;
        if ((termRef.current?.getSelection() ?? "").length > 0) return;
        fitLocal();
      },
      readSize: () => {
        if (!termRef.current) return null;
        return { cols: termRef.current.cols, rows: termRef.current.rows };
      },
      // Ambient sends share the discrete path's dedupe memory and fire-time
      // keyboard check, so the two paths can never double-send one grid.
      sendResize: (cols, rows) => {
        if (!isActive() || !connectionRef.current?.isOpen() || isKeyboardOpen()) return;
        if (cols === lastSentCols && rows === lastSentRows) return;
        lastSentCols = cols;
        lastSentRows = rows;
        connectionRef.current.sendResize(cols, rows);
      },
    });

    const scheduleFit = (resizeWithFit: boolean, discreteIntent = false) => {
      if (!isActive()) return;
      if (isKeyboardOpen() && !discreteIntent) {
        return;
      }
      // term.resize clears selection; skip ambient fits while Copy/selection is live.
      if (!discreteIntent && (termRef.current?.getSelection() ?? "").length > 0) {
        return;
      }
      if (fitFrame) cancelAnimationFrame(fitFrame);
      fitFrame = requestAnimationFrame(() => {
        fitFrame = 0;
        if (!isActive() || (isKeyboardOpen() && !discreteIntent)) return;
        if (!discreteIntent && (termRef.current?.getSelection() ?? "").length > 0) return;
        fitLocal();
        if (resizeWithFit) sendResizeNow(discreteIntent);
      });
    };

    const scheduleImmediate = (discreteIntent = false) => {
      scheduleFit(true, discreteIntent);
    };

    const scheduleDebounced = () => {
      if (!isActive()) return;
      if (isKeyboardOpen()) {
        pendingPostKeyboardResync = true;
        return;
      }
      if (pendingPostKeyboardResync) {
        pendingPostKeyboardResync = false;
        resetDedupe();
      }
      if ((termRef.current?.getSelection() ?? "").length > 0) return;
      refitController?.requestRefit();
    };

    const schedulePostLayout = (discreteIntent = false) => {
      if (!isActive()) return;
      scheduleImmediate(discreteIntent);
    };
    schedulePostLayoutRef.current = schedulePostLayout;

    const onViewportChange = () => {
      syncHostToWrap();
      scheduleDebounced();
    };

    const touchDistance = (touches: TouchList) =>
      Math.hypot(touches[0].clientX - touches[1].clientX, touches[0].clientY - touches[1].clientY);

    const scrollSync = createTerminalScrollSync({
      interactionEl,
      spacerEl,
      getTerminal: () => termRef.current,
      onUnseenOutput: setHasUnseenOutput,
    });

    const onInteractionClick = (event: MouseEvent) => {
      const target = event.target;
      if (target instanceof Element && target.closest("button")) return;
      const textarea = termTextarea();
      if (textarea) {
        resetDocumentScroll();
        textarea.focus({ preventScroll: true });
        // Tap opens (or keeps) the iOS keyboard; settle so inline and fullscreen
        // bands both track the animated visual viewport.
        onBandSettle();
        return;
      }
      termRef.current?.focus();
    };

    jumpToBottomRef.current = () => {
      scrollSync.setFollowLive(true);
      setHasUnseenOutput(false);
      scrollSync.setSyncingScroll(true);
      termRef.current?.scrollToBottom();
      scrollSync.scrollInteractionToBottom();
      scrollSync.setSyncingScroll(false);
      scrollSync.refreshFollow();
    };

    const cancelLongPress = () => {
      longPressActive = false;
      longPressStartedAt = 0;
      if (longPressTimer) {
        clearTimeout(longPressTimer);
        longPressTimer = undefined;
      }
    };

    const stopDirectionalRepeat = () => {
      if (directionalRepeatInterval) {
        clearInterval(directionalRepeatInterval);
        directionalRepeatInterval = undefined;
      }
    };

    const clearDirectionalGesture = () => {
      stopDirectionalRepeat();
      directionalArmed = false;
      directionalArrow = undefined;
    };

    const armDirectionalGesture = (arrow: string, event: TouchEvent) => {
      if (directionalArmed) return;
      // Only steal the gesture when we can actually cancel native pan-y scroll.
      if (!event.cancelable) {
        cancelLongPress();
        return;
      }
      event.preventDefault();
      if (!event.defaultPrevented) {
        cancelLongPress();
        return;
      }
      directionalArmed = true;
      directionalArrow = arrow;
      cancelLongPress();
      sendKey(arrow);
      stopDirectionalRepeat();
      directionalRepeatInterval = setInterval(() => {
        if (directionalArrow) sendKey(directionalArrow);
      }, DIRECTIONAL_REPEAT_INTERVAL_MS);
    };

    const fireLongPressSelect = (clientX: number, clientY: number) => {
      if (longPressSelected) return;
      selectWordAtClient(clientX, clientY);
      longPressSelected = true;
    };

    const isWordChar = (ch: string) => {
      if (!ch) return false;
      const code = ch.charCodeAt(0);
      return (
        (code >= 48 && code <= 57) ||
        (code >= 65 && code <= 90) ||
        (code >= 97 && code <= 122) ||
        code === 45 ||
        code === 95 ||
        code > 127
      );
    };

    const selectWordAtClient = (clientX: number, clientY: number) => {
      if (!termRef.current || !hostEl) return;
      const termEl = termRef.current.element;
      if (!termEl || termRef.current.cols <= 0 || termRef.current.rows <= 0) return;

      const screenEl = termEl.querySelector<HTMLElement>(".xterm-screen");
      const bounds = screenEl?.getBoundingClientRect() ?? hostEl.getBoundingClientRect();
      if (bounds.width <= 0 || bounds.height <= 0) return;

      const relX = clientX - bounds.left;
      const relY = clientY - bounds.top;
      if (relX < 0 || relY < 0 || relX > bounds.width || relY > bounds.height) return;

      const cellWidth = bounds.width / termRef.current.cols;
      const cellHeight = bounds.height / termRef.current.rows;
      const col = Math.min(termRef.current.cols - 1, Math.max(0, Math.floor(relX / cellWidth)));
      const rowInView = Math.min(termRef.current.rows - 1, Math.max(0, Math.floor(relY / cellHeight)));
      const bufferRow = termRef.current.buffer.active.viewportY + rowInView;
      const line = termRef.current.buffer.active.getLine(bufferRow);
      if (!line) return;

      const lineStr = line.translateToString(false);
      const trimmed = lineStr.trimEnd();
      if (!trimmed || col >= trimmed.length) return;

      let start = col;
      while (start > 0 && isWordChar(trimmed[start - 1] ?? "")) start -= 1;
      let end = col;
      while (end < trimmed.length && isWordChar(trimmed[end] ?? "")) end += 1;

      const length = end - start;
      if (length <= 0) return;
      termRef.current.select(start, bufferRow, length);
    };

    const onTouchStart = (event: TouchEvent) => {
      if (event.touches.length === 1) {
        clearDirectionalGesture();
        const touch = event.touches[0];
        longPressStartX = touch.clientX;
        longPressStartY = touch.clientY;
        longPressStartedAt = performance.now();
        longPressActive = true;
        longPressSelected = false;
        if (longPressTimer) {
          clearTimeout(longPressTimer);
          longPressTimer = undefined;
        }
      } else {
        cancelLongPress();
        clearDirectionalGesture();
      }

      if (event.touches.length !== 2) {
        pinchStartDistance = 0;
        pinchEngaged = false;
        return;
      }
      if (event.cancelable) event.preventDefault();
      pinchEngaged = false;
      pinchStartDistance = touchDistance(event.touches);
      pinchBaseFontSize = termRef.current?.options.fontSize ?? DEFAULT_FONT_SIZE;
    };

    const onTouchMove = (event: TouchEvent) => {
      if (directionalArmed) {
        if (event.touches.length !== 1) {
          clearDirectionalGesture();
          cancelLongPress();
        } else if (!event.cancelable) {
          clearDirectionalGesture();
          cancelLongPress();
        } else {
          event.preventDefault();
          if (!event.defaultPrevented) {
            clearDirectionalGesture();
            cancelLongPress();
          }
        }
      } else if (longPressActive) {
        if (event.touches.length !== 1) {
          cancelLongPress();
        } else {
          const touch = event.touches[0];
          const dx = touch.clientX - longPressStartX;
          const dy = touch.clientY - longPressStartY;
          const holdMatured =
            longPressStartedAt > 0 && performance.now() - longPressStartedAt >= LONG_PRESS_MS;
          if (!holdMatured) {
            if (Math.hypot(dx, dy) > LONG_PRESS_MOVE_CANCEL_PX) cancelLongPress();
          } else {
            const absDx = Math.abs(dx);
            const absDy = Math.abs(dy);
            if (Math.max(absDx, absDy) >= DIRECTIONAL_DRAG_THRESHOLD_PX) {
              const arrow =
                absDx > absDy
                  ? dx > 0
                    ? "\x1b[C"
                    : "\x1b[D"
                  : dy > 0
                    ? "\x1b[B"
                    : "\x1b[A";
              armDirectionalGesture(arrow, event);
            }
          }
        }
      }

      if (event.touches.length !== 2 || pinchStartDistance <= 0 || !termRef.current) return;
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
      if (next !== termRef.current.options.fontSize) {
        termRef.current.options.fontSize = next;
        if (!isKeyboardOpen()) fitLocal();
      }
    };

    const onTouchEnd = () => {
      // CI WebKit can delay the 500ms timer past a short hold; still select when
      // the finger lifted after a qualifying hold without movement cancel or
      // directional drag.
      if (
        !directionalArmed &&
        longPressActive &&
        !longPressSelected &&
        longPressStartedAt > 0 &&
        performance.now() - longPressStartedAt >= LONG_PRESS_MS
      ) {
        fireLongPressSelect(longPressStartX, longPressStartY);
      }
      cancelLongPress();
      clearDirectionalGesture();
      if (pinchStartDistance > 0 && pinchEngaged && termRef.current) {
        persistFontSize(termRef.current.options.fontSize ?? DEFAULT_FONT_SIZE);
        resetDedupe();
        schedulePostLayout(true);
      }
      pinchStartDistance = 0;
      pinchEngaged = false;
    };

    const initialFontSize = loadPersistedFontSize();
    const liveTerm = new Terminal({
      fontSize: initialFontSize,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      cursorBlink: true,
      scrollback: 2000,
      theme: {
        background: "#161616",
        foreground: "#e6e6e6",
        cursor: "#87afd7",
      },
    });
    // xterm leaves plain Space keydown uncancelled (keyCode 32 < 48), so the
    // browser page-scrolls the wrap. Own Space here: one PTY frame, no scroll.
    liveTerm.attachCustomKeyEventHandler((event) => {
      if (event.key !== " " && event.code !== "Space") return true;
      if (event.ctrlKey || event.altKey || event.metaKey || event.shiftKey) return true;
      if (event.type === "keydown") {
        event.preventDefault();
        sendKey(" ");
      }
      return false;
    });
    fitAddon = new FitAddon();
    liveTerm.loadAddon(fitAddon);
    liveTerm.open(hostEl);
    onHardenTextarea();
    syncHostToWrap();
    const viteDev =
      (import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV === true;
    if (viteDev) {
      (hostEl as unknown as { __xterm?: Terminal }).__xterm = liveTerm;
    }
    termRef.current = liveTerm;
    const selectionDisposable = liveTerm.onSelectionChange(syncCopyOverlay);
    fitLocal();
    scrollSync.syncSpacer();
    scrollSync.refreshFollow();

    const scrollDisposable = liveTerm.onScroll(scrollSync.onTermScroll);
    interactionEl.addEventListener("scroll", scrollSync.onInteractionScroll, { passive: true });
    interactionEl.addEventListener("click", onInteractionClick);

    const dataDisposable = liveTerm.onData(onTermData);

    interactionEl.addEventListener("touchstart", onTouchStart, { passive: false });
    interactionEl.addEventListener("touchmove", onTouchMove, { passive: false });
    interactionEl.addEventListener("touchend", onTouchEnd, { passive: true });
    interactionEl.addEventListener("touchcancel", onTouchEnd, { passive: true });

    let connection: TerminalConnection | undefined;

    // ponytail: defer dial one microtask so StrictMode's setup→cleanup→setup cycle
    // never constructs a socket on the aborted first mount; cleanup sets `disposed`.
    queueMicrotask(() => {
      if (disposed) return;
      connection = connectTaskTerminal(handle, {
        onOutput: (text) => {
          termRef.current?.write(text, scrollSync.applyOutput);
        },
        onServerError: (message) => {
          setStatusDetail(message);
        },
        onStatus: (next) => {
          setStatus(next);
          if (next === "connected") {
            setStatusDetail("");
          }
        },
        onOpen: (isReconnect, seeded) => {
          setStatusDetail("");
          resetDedupe();
          if (isReconnect && seeded && termRef.current) {
            scrollSync.setFollowLive(true);
            setHasUnseenOutput(false);
            scrollSync.setSyncingScroll(true);
            termRef.current.reset();
            scrollSync.syncSpacer();
            termRef.current.scrollToBottom();
            scrollSync.scrollInteractionToBottom();
            scrollSync.setSyncingScroll(false);
            scrollSync.refreshFollow();
          }
          scheduleImmediate(true);
        },
      });
      connectionRef.current = connection;
    });

    const resizeObserver = new ResizeObserver(onViewportChange);
    resizeObserver.observe(hostEl);
    const panelEl = hostEl.parentElement;
    if (panelEl) resizeObserver.observe(panelEl);
    window.addEventListener("resize", onViewportChange);
    window.addEventListener("orientationchange", onViewportChange);
    const viewport = window.visualViewport;
    viewport?.addEventListener("resize", onViewportChange);

    // Any keyboard-open class edge (open or close), in inline or fullscreen:
    // re-run discreteIntent settle so the band tracks iOS visualViewport animation
    // and exit-from-fullscreen-while-keyboard-up is not a frozen no-op.
    let wasKeyboardOpen = isKeyboardOpen();
    const keyboardClassObserver = new MutationObserver(() => {
      const nowOpen = isKeyboardOpen();
      if (nowOpen === wasKeyboardOpen) return;
      wasKeyboardOpen = nowOpen;
      resetDocumentScroll();
      onBandSettle();
    });
    keyboardClassObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    return () => {
      disposed = true;
      keyboardClassObserver.disconnect();
      cancelExpandSettle();
      cancelLongPress();
      clearDirectionalGesture();
      cancelScheduledWork();
      refitController?.dispose();
      dataDisposable?.dispose();
      scrollDisposable?.dispose();
      selectionDisposable?.dispose();
      if (copyNoticeTimerRef.current) clearTimeout(copyNoticeTimerRef.current);
      interactionEl.removeEventListener("scroll", scrollSync.onInteractionScroll);
      interactionEl.removeEventListener("click", onInteractionClick);
      interactionEl.removeEventListener("touchstart", onTouchStart);
      interactionEl.removeEventListener("touchmove", onTouchMove);
      interactionEl.removeEventListener("touchend", onTouchEnd);
      interactionEl.removeEventListener("touchcancel", onTouchEnd);
      if (ctrlTimerRef.current) clearTimeout(ctrlTimerRef.current);
      resizeObserver?.disconnect();
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("orientationchange", onViewportChange);
      viewport?.removeEventListener("resize", onViewportChange);
      clearExpandedInert();
      document.documentElement.classList.remove(EXPANDED_CLASS);
      connection?.dispose();
      if (connection && connectionRef.current === connection) {
        connectionRef.current = undefined;
      }
      fitAddon?.dispose();
      termRef.current?.dispose();
      if (viteDev && hostEl) {
        delete (hostEl as unknown as { __xterm?: Terminal }).__xterm;
      }
      termRef.current = undefined;
      hostEl.style.height = "";
      resetResizeDedupeRef.current = undefined;
      schedulePostLayoutRef.current = undefined;
      jumpToBottomRef.current = undefined;
    };
  }, [handle]);

  return (
    <section
      className={`terminal-panel${expanded ? " is-expanded" : ""}`}
      data-testid="task-terminal-panel"
      aria-label="Task terminal">
      <div
        className="terminal-interaction-wrap"
        data-testid="terminal-interaction-surface"
        ref={interactionElRef}>
        <div className="terminal-host" ref={hostElRef}></div>
        <div className="terminal-scroll-spacer" ref={spacerElRef} aria-hidden="true"></div>
        {hasUnseenOutput ? (
          <button
            type="button"
            className="terminal-new-output"
            onClick={() => jumpToBottomRef.current?.()}>
            New output ↓
          </button>
        ) : null}
      </div>
      {copyNotice ? (
        <p className="terminal-copy-notice" role="status">
          {copyNotice}
        </p>
      ) : null}
      <div className="terminal-corner-actions">
        {copyOverlayText ? (
          <button
            type="button"
            className="terminal-copy-overlay"
            data-testid="terminal-copy-overlay"
            onClick={() => void copySelection()}>
            Copy
          </button>
        ) : null}
        <button
          type="button"
          className={`terminal-expand-corner${expanded ? " is-armed" : ""}`}
          aria-label="Expand terminal"
          aria-pressed={expanded}
          onPointerDown={(event) => event.preventDefault()}
          onClick={() => toggleExpanded()}>
          ⛶
        </button>
      </div>
      <div
        className={`terminal-status${statusVisible ? "" : " is-empty"}`}
        data-testid="terminal-status"
        aria-hidden={statusVisible ? "false" : "true"}>
        {statusVisible ? (
          <>
            <span className="terminal-status-label">{STATUS_LABELS[status]}</span>
            {statusDetail ? (
              <span className="terminal-status-detail">{statusDetail}</span>
            ) : null}
            {showReconnect ? (
              <button
                type="button"
                className="terminal-status-reconnect"
                onClick={() => requestReconnect()}>
                Reconnect
              </button>
            ) : null}
          </>
        ) : null}
      </div>
      {copyFallbackOpen ? (
        <div className="terminal-paste-fallback">
          <p className="terminal-paste-notice" role="status">
            Clipboard unavailable — copy below.
          </p>
          <textarea
            className="terminal-paste-input"
            readOnly
            aria-label="Copy text"
            value={copyFallbackText}></textarea>
          <div className="terminal-paste-actions">
            <button type="button" className="terminal-key" onClick={() => dismissCopyFallback()}>
              Done
            </button>
          </div>
        </div>
      ) : null}
      {pasteFallbackOpen ? (
        <div className="terminal-paste-fallback">
          <p className="terminal-paste-notice" role="status">
            {pasteNotice}
          </p>
          <textarea
            className="terminal-paste-input"
            aria-label="Paste text"
            value={pasteFallbackText}
            onChange={(event) => setPasteFallbackText(event.target.value)}></textarea>
          <div className="terminal-paste-actions">
            <button type="button" className="terminal-key" onClick={() => sendPasteFallback()}>
              Send
            </button>
            <button type="button" className="terminal-key" onClick={() => cancelPasteFallback()}>
              Cancel
            </button>
          </div>
        </div>
      ) : null}
      <div data-testid="terminal-bottom-controls">
        <div className="terminal-keys" role="toolbar" aria-label="Terminal keys">
          {CONTROL_KEYS.map((key) => (
            <button
              key={key.label}
              type="button"
              className="terminal-key"
              aria-label={key.ariaLabel}
              onPointerDown={onToolbarPointerDown}
              onClick={(event) => {
                const ownedFocus = consumeToolbarPointerOwnedFocus(event);
                sendKey(consumeCtrl(key.data));
                refocusTermIfOwned(ownedFocus);
              }}>
              {key.label}
            </button>
          ))}
          <button
            type="button"
            className={`terminal-key${ctrlArmed ? " is-armed" : ""}`}
            aria-label="Control modifier"
            aria-pressed={ctrlArmed}
            onPointerDown={onToolbarPointerDown}
            onClick={(event) => {
              const ownedFocus = consumeToolbarPointerOwnedFocus(event);
              toggleCtrl();
              refocusTermIfOwned(ownedFocus);
            }}>
            Ctrl
            {ctrlArmed ? (
              <span className="terminal-key-armed-dot" aria-hidden="true"></span>
            ) : null}
          </button>
          <button
            type="button"
            className="terminal-key"
            aria-label="Paste"
            onPointerDown={onToolbarPointerDown}
            onClick={(event) => {
              const ownedFocus = consumeToolbarPointerOwnedFocus(event);
              void requestPaste(ownedFocus);
            }}>
            Paste
          </button>
        </div>
      </div>
    </section>
  );
}
