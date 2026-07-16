<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { copyText } from "../diagnostics";
  import { resetDocumentScroll } from "../viewport";
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
  let pasteFallbackOpen = $state(false);
  let pasteFallbackText = $state("");
  let pasteNotice = $state("");
  let copyOverlayText = $state("");
  let copyNotice = $state("");
  let copyFallbackOpen = $state(false);
  let copyFallbackText = $state("");
  let pasteFallbackOwnedFocus = false;
  let copyNoticeTimer: ReturnType<typeof setTimeout> | undefined;
  let inertedElements: HTMLElement[] = [];

  const isPhoneTerminalLayout = () =>
    window.matchMedia("(max-width: 767px), (pointer: coarse) and (max-height: 500px)").matches;

  const clearExpandedInert = () => {
    for (const el of inertedElements) {
      el.inert = false;
    }
    inertedElements = [];
  };

  const applyExpandedInert = () => {
    clearExpandedInert();
    if (!isPhoneTerminalLayout()) return;

    const panel = hostEl?.closest<HTMLElement>('[data-testid="task-terminal-panel"]');
    const taskDetail = panel?.parentElement;
    const next: HTMLElement[] = [];

    if (taskDetail) {
      for (const child of taskDetail.children) {
        if (child instanceof HTMLElement && child !== panel) {
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
      inertedElements.push(el);
    }
  };

  const syncExpandedInert = (active: boolean) => {
    if (active) applyExpandedInert();
    else clearExpandedInert();
  };

  const MIN_TERMINAL_COLS = 80;
  const RESIZE_DEBOUNCE_MS = 100;
  const EXPAND_REWRAP_MS = 280;
  const EXPANDED_CLASS = "terminal-expanded";
  const FONT_STORAGE_KEY = "ajax.terminal.fontSize";
  const DEFAULT_FONT_SIZE = 13;
  const MIN_FONT_SIZE = 7;
  const MAX_FONT_SIZE = 20;
  const PINCH_ACTIVATION_PX = 12;
  const LONG_PRESS_MS = 500;
  const LONG_PRESS_MOVE_CANCEL_PX = 8;

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
  let schedulePostLayoutRef: ((discreteIntent?: boolean) => void) | undefined;
  let jumpToBottomRef: (() => void) | undefined;

  let expandSettleFrame1 = 0;
  let expandSettleFrame2 = 0;
  let expandSettleTimer: ReturnType<typeof setTimeout> | undefined;

  const cancelExpandSettle = () => {
    if (expandSettleFrame1) {
      cancelAnimationFrame(expandSettleFrame1);
      expandSettleFrame1 = 0;
    }
    if (expandSettleFrame2) {
      cancelAnimationFrame(expandSettleFrame2);
      expandSettleFrame2 = 0;
    }
    if (expandSettleTimer) {
      clearTimeout(expandSettleTimer);
      expandSettleTimer = undefined;
    }
  };

  const scheduleBandSettle = () => {
    cancelExpandSettle();
    schedulePostLayoutRef?.(true);
    expandSettleFrame1 = requestAnimationFrame(() => {
      expandSettleFrame1 = 0;
      schedulePostLayoutRef?.(true);
      expandSettleFrame2 = requestAnimationFrame(() => {
        expandSettleFrame2 = 0;
        schedulePostLayoutRef?.(true);
      });
    });
    expandSettleTimer = setTimeout(() => {
      expandSettleTimer = undefined;
      schedulePostLayoutRef?.(true);
    }, EXPAND_REWRAP_MS);
  };

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

  const PASTE_DISCONNECTED_NOTICE = "Terminal disconnected — paste kept below.";

  const pasteToPty = (text: string): boolean => {
    if (!text || !connection?.isOpen()) return false;
    const payload = term?.modes.bracketedPasteMode
      ? `\x1b[200~${text}\x1b[201~`
      : text;
    connection.sendInput(payload);
    return true;
  };

  const termTextarea = (): HTMLTextAreaElement | null => {
    const el = term?.element?.querySelector("textarea.xterm-helper-textarea");
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
    term?.focus();
  };

  const blurTerm = () => {
    termTextarea()?.blur();
  };

  let toolbarPointerOwnedFocus = false;

  const onToolbarPointerDown = (event: PointerEvent) => {
    event.preventDefault();
    toolbarPointerOwnedFocus = termOwnedFocus();
  };

  const consumeToolbarPointerOwnedFocus = (event: MouseEvent): boolean => {
    const owned = toolbarPointerOwnedFocus && event.detail !== 0;
    toolbarPointerOwnedFocus = false;
    return owned;
  };

  const openPasteFallback = (ownedFocus: boolean, notice: string, text = "") => {
    pasteFallbackOwnedFocus = ownedFocus;
    pasteNotice = notice;
    pasteFallbackText = text;
    pasteFallbackOpen = true;
  };

  const retainUnsentPaste = (text: string, ownedFocus: boolean) => {
    openPasteFallback(ownedFocus, PASTE_DISCONNECTED_NOTICE, text);
  };

  const dismissPasteFallback = (): boolean => {
    const ownedFocus = pasteFallbackOwnedFocus;
    pasteFallbackOpen = false;
    pasteFallbackText = "";
    pasteNotice = "";
    pasteFallbackOwnedFocus = false;
    return ownedFocus;
  };

  const closePasteFallback = () => {
    refocusTermIfOwned(dismissPasteFallback());
  };

  const pasteThroughTerm = (text: string, ownedFocus = true): boolean => {
    if (!text || !term) return false;
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
    const ownedFocus = pasteFallbackOwnedFocus;
    if (!text) {
      closePasteFallback();
      return;
    }
    if (!pasteToPty(text)) {
      pasteNotice = PASTE_DISCONNECTED_NOTICE;
      return;
    }
    dismissPasteFallback();
    refocusTermIfOwned(ownedFocus);
  };

  const cancelPasteFallback = () => {
    closePasteFallback();
  };

  const syncCopyOverlay = () => {
    copyOverlayText = term?.getSelection() ?? "";
    if (!copyOverlayText && !copyNoticeTimer) copyNotice = "";
  };

  const dismissCopyFallback = () => {
    copyFallbackOpen = false;
    copyFallbackText = "";
  };

  const copySelection = async () => {
    const text = copyOverlayText || term?.getSelection() || "";
    if (!text) return;
    const copied = await copyText(text);
    if (copied) {
      if (copyNoticeTimer) clearTimeout(copyNoticeTimer);
      copyNotice = "Copied";
      copyNoticeTimer = setTimeout(() => {
        copyNotice = "";
        copyNoticeTimer = undefined;
      }, 1500);
      term?.clearSelection();
      copyOverlayText = "";
      return;
    }
    copyFallbackText = text;
    copyFallbackOpen = true;
  };

  const requestReconnect = () => {
    connection?.reconnectNow();
  };

  const toggleExpanded = () => {
    const entering = !expanded;
    expanded = entering;
    document.documentElement.classList.toggle(EXPANDED_CLASS, expanded);
    syncExpandedInert(entering);
    resetResizeDedupe?.();
    if (!entering) {
      blurTerm();
      // Exit while keyboard-open used to call discreteIntent=false, which is a
      // no-op under the fit freeze — inline band never refit. Always settle.
      scheduleBandSettle();
      return;
    }
    scheduleBandSettle();
  };

  onMount(() => {
    let fitAddon: FitAddon | undefined;
    let dataDisposable: { dispose(): void } | undefined;
    let scrollDisposable: { dispose(): void } | undefined;
    let selectionDisposable: { dispose(): void } | undefined;
    let resizeObserver: ResizeObserver | undefined;
    let lastSentCols = 0;
    let lastSentRows = 0;
    let resizeTimer: ReturnType<typeof setTimeout> | undefined;
    let keyboardResizeTimer: ReturnType<typeof setTimeout> | undefined;
    let fitFrame = 0;
    let pendingPostKeyboardResync = false;
    let disposed = false;
    let followLive = true;
    let pinchStartDistance = 0;
    let pinchBaseFontSize = DEFAULT_FONT_SIZE;
    let pinchEngaged = false;
    let longPressTimer: ReturnType<typeof setTimeout> | undefined;
    let longPressStartX = 0;
    let longPressStartY = 0;
    let longPressStartedAt = 0;
    let longPressActive = false;
    let longPressSelected = false;
    let syncingScroll = false;
    let wrapperDroveScroll = false;

    const isKeyboardOpen = () => document.documentElement.classList.contains("keyboard-open");

    const isActive = () => !disposed;

    const cancelScheduledWork = () => {
      if (fitFrame) {
        cancelAnimationFrame(fitFrame);
        fitFrame = 0;
      }
      if (resizeTimer) {
        clearTimeout(resizeTimer);
        resizeTimer = undefined;
      }
      if (keyboardResizeTimer) {
        clearTimeout(keyboardResizeTimer);
        keyboardResizeTimer = undefined;
      }
    };

    const resetDedupe = () => {
      lastSentCols = 0;
      lastSentRows = 0;
    };
    resetResizeDedupe = resetDedupe;

    const sendResizeNow = (discreteIntent = false) => {
      if (!isActive() || !connection?.isOpen() || !term) return;
      if (isKeyboardOpen() && !discreteIntent) return;
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
      if (!isActive() || !fitAddon || !term || !hostEl) return;
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

      const currentFontSize = term.options.fontSize ?? DEFAULT_FONT_SIZE;
      const logicalCols =
        MIN_TERMINAL_COLS + Math.max(0, MAX_FONT_SIZE - currentFontSize);
      const scale = Math.min(1, hostWidth / (logicalCols * cellWidth));
      const logicalRows = Math.max(1, Math.ceil(hostHeight / (cellHeight * scale)));

      term.resize(logicalCols, logicalRows);
      termEl.style.width = `${hostWidth / scale}px`;
      termEl.style.height = `${hostHeight / scale}px`;
      termEl.style.transformOrigin = "0 0";
      termEl.style.transform = `scale(${scale})`;
    };

    const scheduleFit = (resizeWithFit: boolean, discreteIntent = false) => {
      if (!isActive()) return;
      if (isKeyboardOpen() && !discreteIntent) {
        return;
      }
      if (fitFrame) cancelAnimationFrame(fitFrame);
      fitFrame = requestAnimationFrame(() => {
        fitFrame = 0;
        if (!isActive() || (isKeyboardOpen() && !discreteIntent)) return;
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
      scheduleFit(false, false);
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        resizeTimer = undefined;
        if (!isActive() || isKeyboardOpen()) return;
        sendResizeNow(false);
      }, RESIZE_DEBOUNCE_MS);
    };

    const schedulePostLayout = (discreteIntent = false) => {
      if (!isActive()) return;
      scheduleImmediate(discreteIntent);
    };
    schedulePostLayoutRef = schedulePostLayout;

    const onViewportChange = () => {
      if (isKeyboardOpen()) {
        if (keyboardResizeTimer) clearTimeout(keyboardResizeTimer);
        keyboardResizeTimer = setTimeout(() => {
          keyboardResizeTimer = undefined;
          if (!isActive() || !isKeyboardOpen()) return;
          scheduleImmediate(true);
        }, RESIZE_DEBOUNCE_MS);
        return;
      }
      scheduleDebounced();
    };

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

    const onInteractionClick = (event: MouseEvent) => {
      const target = event.target;
      if (target instanceof Element && target.closest("button")) return;
      const textarea = termTextarea();
      if (textarea) {
        resetDocumentScroll();
        textarea.focus({ preventScroll: true });
        // Tap opens (or keeps) the iOS keyboard; settle so inline and fullscreen
        // bands both track the animated visual viewport.
        scheduleBandSettle();
        return;
      }
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

    const cancelLongPress = () => {
      longPressActive = false;
      longPressStartedAt = 0;
      if (longPressTimer) {
        clearTimeout(longPressTimer);
        longPressTimer = undefined;
      }
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
      if (!term || !hostEl) return;
      const termEl = term.element;
      if (!termEl || term.cols <= 0 || term.rows <= 0) return;

      const screenEl = termEl.querySelector<HTMLElement>(".xterm-screen");
      const bounds = screenEl?.getBoundingClientRect() ?? hostEl.getBoundingClientRect();
      if (bounds.width <= 0 || bounds.height <= 0) return;

      const relX = clientX - bounds.left;
      const relY = clientY - bounds.top;
      if (relX < 0 || relY < 0 || relX > bounds.width || relY > bounds.height) return;

      const cellWidth = bounds.width / term.cols;
      const cellHeight = bounds.height / term.rows;
      const col = Math.min(term.cols - 1, Math.max(0, Math.floor(relX / cellWidth)));
      const rowInView = Math.min(term.rows - 1, Math.max(0, Math.floor(relY / cellHeight)));
      const bufferRow = term.buffer.active.viewportY + rowInView;
      const line = term.buffer.active.getLine(bufferRow);
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
      term.select(start, bufferRow, length);
    };

    const onTouchStart = (event: TouchEvent) => {
      if (event.touches.length === 1) {
        const touch = event.touches[0];
        longPressStartX = touch.clientX;
        longPressStartY = touch.clientY;
        longPressStartedAt = performance.now();
        longPressActive = true;
        longPressSelected = false;
        if (longPressTimer) clearTimeout(longPressTimer);
        longPressTimer = setTimeout(() => {
          longPressTimer = undefined;
          if (!longPressActive || !term) return;
          fireLongPressSelect(longPressStartX, longPressStartY);
          longPressActive = false;
        }, LONG_PRESS_MS);
      } else {
        cancelLongPress();
      }

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
      if (longPressActive) {
        if (event.touches.length !== 1) {
          cancelLongPress();
        } else {
          const touch = event.touches[0];
          const dx = touch.clientX - longPressStartX;
          const dy = touch.clientY - longPressStartY;
          if (Math.hypot(dx, dy) > LONG_PRESS_MOVE_CANCEL_PX) cancelLongPress();
        }
      }

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
        if (!isKeyboardOpen()) fitLocal();
      }
    };

    const onTouchEnd = () => {
      // CI WebKit can delay the 500ms timer past a short hold; still select when
      // the finger lifted after a qualifying hold without movement cancel.
      if (
        longPressActive &&
        !longPressSelected &&
        longPressStartedAt > 0 &&
        performance.now() - longPressStartedAt >= LONG_PRESS_MS
      ) {
        fireLongPressSelect(longPressStartX, longPressStartY);
      }
      cancelLongPress();
      if (pinchStartDistance > 0 && pinchEngaged && term) {
        persistFontSize(term.options.fontSize ?? DEFAULT_FONT_SIZE);
        resetDedupe();
        schedulePostLayout(true);
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
    hardenMobileTextarea();
    const viteDev =
      (import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV === true;
    if (viteDev) {
      (hostEl as unknown as { __xterm?: Terminal }).__xterm = liveTerm;
    }
    term = liveTerm;
    selectionDisposable = liveTerm.onSelectionChange(syncCopyOverlay);
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
      onOpen: (isReconnect, seeded) => {
        statusDetail = "";
        resetDedupe();
        if (isReconnect && seeded && term) {
          followLive = true;
          hasUnseenOutput = false;
          syncingScroll = true;
          term.reset();
          syncSpacer();
          term.scrollToBottom();
          scrollInteractionToBottom();
          syncingScroll = false;
          refreshFollow();
        }
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

    // Any keyboard-open class edge (open or close), in inline or fullscreen:
    // re-run discreteIntent settle so the band tracks iOS visualViewport animation
    // and exit-from-fullscreen-while-keyboard-up is not a frozen no-op.
    let wasKeyboardOpen = isKeyboardOpen();
    const keyboardClassObserver = new MutationObserver(() => {
      const nowOpen = isKeyboardOpen();
      if (nowOpen === wasKeyboardOpen) return;
      wasKeyboardOpen = nowOpen;
      resetDocumentScroll();
      scheduleBandSettle();
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
      cancelScheduledWork();
      dataDisposable?.dispose();
      scrollDisposable?.dispose();
      selectionDisposable?.dispose();
      if (copyNoticeTimer) clearTimeout(copyNoticeTimer);
      if (interactionEl) {
        interactionEl.removeEventListener("scroll", onInteractionScroll);
        interactionEl.removeEventListener("click", onInteractionClick);
        interactionEl.removeEventListener("touchstart", onTouchStart);
        interactionEl.removeEventListener("touchmove", onTouchMove);
        interactionEl.removeEventListener("touchend", onTouchEnd);
        interactionEl.removeEventListener("touchcancel", onTouchEnd);
      }
      if (ctrlTimer) clearTimeout(ctrlTimer);
      resizeObserver?.disconnect();
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("orientationchange", onViewportChange);
      viewport?.removeEventListener("resize", onViewportChange);
      clearExpandedInert();
      document.documentElement.classList.remove(EXPANDED_CLASS);
      connection?.dispose();
      fitAddon?.dispose();
      term?.dispose();
      if (viteDev && hostEl) {
        delete (hostEl as unknown as { __xterm?: Terminal }).__xterm;
      }
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
  {#if copyNotice}
    <p class="terminal-copy-notice" role="status">{copyNotice}</p>
  {/if}
  <div class="terminal-corner-actions">
    {#if copyOverlayText}
      <button
        type="button"
        class="terminal-copy-overlay"
        data-testid="terminal-copy-overlay"
        onclick={() => void copySelection()}>Copy</button>
    {/if}
    <button
      type="button"
      class="terminal-expand-corner"
      class:is-armed={expanded}
      aria-label="Expand terminal"
      aria-pressed={expanded}
      onpointerdown={(event) => event.preventDefault()}
      onclick={() => toggleExpanded()}>⛶</button>
  </div>
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
  {#if copyFallbackOpen}
    <div class="terminal-paste-fallback">
      <p class="terminal-paste-notice" role="status">Clipboard unavailable — copy below.</p>
      <textarea
        class="terminal-paste-input"
        readonly
        aria-label="Copy text"
        value={copyFallbackText}></textarea>
      <div class="terminal-paste-actions">
        <button type="button" class="terminal-key" onclick={() => dismissCopyFallback()}>Done</button>
      </div>
    </div>
  {/if}
  {#if pasteFallbackOpen}
    <div class="terminal-paste-fallback">
      <p class="terminal-paste-notice" role="status">{pasteNotice}</p>
      <textarea
        class="terminal-paste-input"
        aria-label="Paste text"
        bind:value={pasteFallbackText}></textarea>
      <div class="terminal-paste-actions">
        <button type="button" class="terminal-key" onclick={() => sendPasteFallback()}>Send</button>
        <button type="button" class="terminal-key" onclick={() => cancelPasteFallback()}>Cancel</button>
      </div>
    </div>
  {/if}
  <div data-testid="terminal-bottom-controls">
    <div class="terminal-keys" role="toolbar" aria-label="Terminal keys">
      {#each CONTROL_KEYS as key (key.label)}
        <button
          type="button"
          class="terminal-key"
          onpointerdown={onToolbarPointerDown}
          onclick={(event) => {
            const ownedFocus = consumeToolbarPointerOwnedFocus(event);
            sendKey(consumeCtrl(key.data));
            refocusTermIfOwned(ownedFocus);
          }}>{key.label}</button>
      {/each}
      <button
        type="button"
        class="terminal-key"
        class:is-armed={ctrlArmed}
        aria-pressed={ctrlArmed}
        onpointerdown={onToolbarPointerDown}
        onclick={(event) => {
          const ownedFocus = consumeToolbarPointerOwnedFocus(event);
          toggleCtrl();
          refocusTermIfOwned(ownedFocus);
        }}>Ctrl{#if ctrlArmed}<span class="terminal-key-armed-dot" aria-hidden="true"></span>{/if}</button>
      <button
        type="button"
        class="terminal-key"
        onpointerdown={onToolbarPointerDown}
        onclick={(event) => {
          const ownedFocus = consumeToolbarPointerOwnedFocus(event);
          void requestPaste(ownedFocus);
        }}>Paste</button>
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
    -ms-overflow-style: none;
    scrollbar-width: none;
  }

  .terminal-interaction-wrap::-webkit-scrollbar {
    display: none;
    width: 0;
    height: 0;
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

  .terminal-host :global(textarea.xterm-helper-textarea) {
    position: absolute !important;
    left: 0 !important;
    top: auto !important;
    bottom: 0 !important;
    height: 44px !important;
    width: 100% !important;
    opacity: 0.01 !important;
    clip-path: none !important;
    -webkit-clip-path: none !important;
    color: transparent;
    -webkit-text-fill-color: transparent;
    caret-color: transparent;
    z-index: 1;
  }

  .terminal-paste-fallback {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 8px;
    border-top: 1px solid var(--rule);
    background: var(--paper-raised);
  }

  .terminal-paste-notice {
    margin: 0;
    font-size: 12px;
    color: var(--ink-muted);
  }

  .terminal-paste-input {
    width: 100%;
    min-height: 72px;
    padding: 6px 8px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--paper);
    color: var(--ink);
    font-family: var(--mono);
    font-size: 12px;
    resize: vertical;
  }

  .terminal-paste-actions {
    display: flex;
    gap: 6px;
  }

  .terminal-copy-notice {
    position: absolute;
    left: 50%;
    top: 8px;
    transform: translateX(-50%);
    z-index: 7;
    margin: 0;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--paper-raised);
    border: 1px solid var(--rule-strong);
    font-size: 12px;
    font-weight: 600;
    color: var(--ink);
  }

  /* Panel-scoped (not inside the scroll wrap) so selection Copy stays visible
     next to expand instead of at 50% of tall scroll content. */
  .terminal-corner-actions {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 8;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .terminal-copy-overlay {
    min-width: 44px;
    min-height: 44px;
    padding: 6px 14px;
    border: 1px solid var(--rule-strong);
    border-radius: 999px;
    background: var(--paper-raised);
    color: var(--ink);
    font-size: 12px;
    font-weight: 600;
  }

  .terminal-new-output {
    position: absolute;
    left: 50%;
    bottom: 8px;
    transform: translateX(-50%);
    z-index: 6;
    min-height: 44px;
    padding: 6px 12px;
    border: 1px solid var(--rule-strong);
    border-radius: 999px;
    background: var(--paper-raised);
    color: var(--ink);
    font-size: 12px;
    font-weight: 600;
  }

  .terminal-expand-corner {
    min-width: 44px;
    min-height: 44px;
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
    min-height: 44px;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--rule);
    background: var(--paper-raised);
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
    min-width: 44px;
    min-height: 44px;
    padding: 1px 7px;
    font-size: 11px;
    border-radius: 6px;
    border: 1px solid var(--rule);
    background: var(--paper-raised);
    color: var(--ink);
  }

  .terminal-keys .terminal-key {
    min-width: 32px;
    min-height: 32px;
    padding: 1px 5px;
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

  @media (min-width: 768px) and (not ((pointer: coarse) and (max-height: 500px))) {
    .terminal-panel .terminal-interaction-wrap {
      height: min(58vh, 560px);
    }

    .terminal-panel .terminal-host {
      height: 100%;
    }
  }

  @media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {
    .terminal-panel:not(.is-expanded) .terminal-interaction-wrap {
      display: flex;
      flex-direction: column;
      flex: 1 1 auto;
      min-height: 0;
      height: auto;
    }

    .terminal-panel:not(.is-expanded) .terminal-host {
      flex: 1 1 auto;
      min-height: 0;
      height: auto;
    }

    :global([data-testid="terminal-bottom-controls"]) {
      flex: none;
      width: 100%;
    }

    .terminal-keys {
      width: 100%;
      padding-bottom: max(2px, env(safe-area-inset-bottom));
    }

    .terminal-keys .terminal-key {
      flex: 1 1 0;
      min-width: 0;
    }

    :global(html.keyboard-open) .terminal-panel:not(.is-expanded) .terminal-interaction-wrap {
      height: auto;
      flex: 1 1 auto;
      min-height: 0;
    }

    :global(html.keyboard-open) .terminal-panel:not(.is-expanded) :global([data-testid="terminal-bottom-controls"]) {
      flex: none;
    }

    :global(html.keyboard-open) .terminal-keys {
      padding-bottom: 6px;
    }

    :global(html.terminal-expanded) .terminal-panel.is-expanded {
      position: fixed;
      /* Height from --app-top / --app-height (visualViewport). */
      top: var(--app-top, var(--app-band-top, 0px));
      right: 0;
      left: 0;
      z-index: 45;
      display: flex;
      flex-direction: column;
      height: var(--app-height, var(--app-band-height, 100dvh));
      max-height: var(--app-height, var(--app-band-height, 100dvh));
      min-height: 0;
      margin-top: 0;
      box-sizing: border-box;
      overflow: hidden;
      background: var(--paper);
      border-left: none;
      border-right: none;
      border-bottom: none;
      border-radius: 0;
    }

    .terminal-panel.is-expanded .terminal-interaction-wrap {
      flex: 1 1 auto;
      min-height: 0;
      height: auto;
    }

    .terminal-panel.is-expanded :global([data-testid="terminal-bottom-controls"]) {
      flex: none;
    }

    .terminal-panel.is-expanded .terminal-host {
      height: 100%;
      min-height: 0;
    }
  }
</style>
