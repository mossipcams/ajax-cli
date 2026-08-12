import type { Dispatch, MutableRefObject, RefObject, SetStateAction } from "react";
import { attachTerminalAddons } from "@/shared/lib/terminalAddons";
import { findHttpLinkAtClient } from "@/shared/lib/terminalLinkHitTest";
import type { TerminalLinkService } from "@/shared/lib/terminalLinkService";
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
  terminalScrollbackLines,
} from "@/shared/lib/terminalGeometry";
import { createRefitController } from "@/shared/lib/terminalRefit";
import { detectCsiEraseInDisplay } from "@/shared/lib/detectCsiEraseInDisplay";
import { createTerminalScrollSync } from "@/shared/lib/terminalScrollSync";
import { Terminal } from "@xterm/xterm";
import {
  clientToBufferCell,
  selectRangeFromWordAnchor,
  selectWordAtClient,
  wordBoundsAtCol,
} from "./terminalTouchSelection";
import { setTerminalDoubleTapPending, setTerminalSelecting } from "@/shared/lib/terminalSelecting";

/**
 * Quiet time after the last seeded-open write before the terminal is revealed.
 * Floor is the bridge's 16ms output batch (TERMINAL_OUTPUT_FLUSH_MS) plus link
 * jitter; ~7 batches is enough to bridge seed → attach repaint without sitting
 * on a blank plate.
 */
const SEED_REVEAL_QUIET_MS = 120;
/** Hard cap so a pane streaming nonstop still reveals. */
const SEED_REVEAL_MAX_MS = 2000;
/** Force scrollOnErase off if no post-reveal CSI erase is seen (split-chunk safe). */
const POST_REVEAL_ERASE_GRACE_MS = 1000;

const EXPANDED_CLASS = "terminal-expanded";
const PINCH_ACTIVATION_PX = 12;
const LONG_PRESS_MS = 500;
const LONG_PRESS_MOVE_CANCEL_PX = 8;
const DIRECTIONAL_DRAG_THRESHOLD_PX = 24;
const DIRECTIONAL_REPEAT_INTERVAL_MS = 75;
const DOUBLE_TAP_WINDOW_MS = 350;
const DOUBLE_TAP_SLOP_PX = 24;

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

export type MountTaskTerminalSessionDeps = {
  handle: string;
  hostElRef: RefObject<HTMLDivElement | null>;
  interactionElRef: RefObject<HTMLDivElement | null>;
  spacerElRef: RefObject<HTMLDivElement | null>;
  termRef: MutableRefObject<Terminal | undefined>;
  connectionRef: MutableRefObject<TerminalConnection | undefined>;
  schedulePostLayoutRef: MutableRefObject<((discreteIntent?: boolean) => void) | undefined>;
  resetResizeDedupeRef: MutableRefObject<(() => void) | undefined>;
  jumpToBottomRef: MutableRefObject<(() => void) | undefined>;
  linkServiceRef: MutableRefObject<TerminalLinkService | undefined>;
  terminalSnapshotRef: MutableRefObject<
    ReturnType<typeof attachTerminalAddons>["snapshot"] | undefined
  >;
  copyNoticeTimerRef: MutableRefObject<ReturnType<typeof setTimeout> | undefined>;
  ctrlTimerRef: MutableRefObject<ReturnType<typeof setTimeout> | undefined>;
  setStatus: Dispatch<SetStateAction<TerminalConnectionStatus>>;
  setStatusDetail: Dispatch<SetStateAction<string>>;
  setHasUnseenOutput: Dispatch<SetStateAction<boolean>>;
  setLinkMenu: Dispatch<
    SetStateAction<{ url: string; x: number; y: number } | null>
  >;
  syncCopyOverlay: () => void;
  sendKey: (data: string) => void;
  termTextarea: () => HTMLTextAreaElement | null;
  cancelExpandSettle: () => void;
  clearExpandedInert: () => void;
  cancelSpeechTransport: () => void;
  seedSentinelFromFocus: (event: Event) => void;
  onHardenTextarea: () => void;
  onBandSettle: () => void;
  onTermData: (data: string) => void;
  onBeforeInput: (event: InputEvent) => void;
  onInputEvent: (event: Event) => void;
  onPaste: (event: ClipboardEvent) => void;
  onSeedTermSentinel: () => void;
  onRestorePinnedScroll: () => boolean;
};

export function mountTaskTerminalSession(
  deps: MountTaskTerminalSessionDeps,
): () => void {
  const {
    handle,
    hostElRef,
    interactionElRef,
    spacerElRef,
    termRef,
    connectionRef,
    schedulePostLayoutRef,
    resetResizeDedupeRef,
    jumpToBottomRef,
    linkServiceRef,
    terminalSnapshotRef,
    copyNoticeTimerRef,
    ctrlTimerRef,
    setStatus,
    setStatusDetail,
    setHasUnseenOutput,
    setLinkMenu,
    syncCopyOverlay,
    sendKey,
    termTextarea,
    cancelExpandSettle,
    clearExpandedInert,
    cancelSpeechTransport,
    seedSentinelFromFocus,
    onHardenTextarea,
    onBandSettle,
    onTermData,
    onBeforeInput,
    onInputEvent,
    onPaste,
    onSeedTermSentinel,
    onRestorePinnedScroll,
  } = deps;

  const hostEl = hostElRef.current;
  const interactionEl = interactionElRef.current;
  const spacerEl = spacerElRef.current;
  if (!hostEl || !interactionEl || typeof window.matchMedia !== "function") {
    return () => {};
  }

  // Deferred init: closed over by fitLocal / cleanup before first assignment.
  // eslint-disable-next-line prefer-const -- assigned once after helper closures are built
  let terminalAddons: ReturnType<typeof attachTerminalAddons> | undefined;
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
  let pendingTapX = 0;
  let pendingTapY = 0;
  let pendingTapAt = 0;
  let selectionDragActive = false;
  let selectionAnchorCol = 0;
  let selectionAnchorRow = 0;
  let selectionAnchorWordEnd = 0;
  let selectionDragLastCol = -1;
  let selectionDragLastRow = -1;
  let seedQuietTimer: ReturnType<typeof setTimeout> | undefined;
  let seedCapTimer: ReturnType<typeof setTimeout> | undefined;
  let postRevealEraseGraceTimer: ReturnType<typeof setTimeout> | undefined;
  let eraseCarry = "";

  const clearPostRevealEraseGraceTimer = () => {
    if (postRevealEraseGraceTimer) clearTimeout(postRevealEraseGraceTimer);
    postRevealEraseGraceTimer = undefined;
  };

  const latchScrollOnEraseOff = () => {
    clearPostRevealEraseGraceTimer();
    if (termRef.current?.options.scrollOnEraseInDisplay) {
      termRef.current.options.scrollOnEraseInDisplay = false;
    }
  };

  const armPostRevealEraseGrace = () => {
    clearPostRevealEraseGraceTimer();
    postRevealEraseGraceTimer = setTimeout(
      latchScrollOnEraseOff,
      POST_REVEAL_ERASE_GRACE_MS,
    );
  };

  const clearSeedPendingRevealTimer = () => {
    if (seedQuietTimer) clearTimeout(seedQuietTimer);
    if (seedCapTimer) clearTimeout(seedCapTimer);
    seedQuietTimer = undefined;
    seedCapTimer = undefined;
  };

  const isSeedPending = () => interactionEl.classList.contains("is-seed-pending");

  const cancelSeedPending = () => {
    clearSeedPendingRevealTimer();
    clearPostRevealEraseGraceTimer();
    eraseCarry = "";
    interactionEl.classList.remove("is-seed-pending");
  };

  let revealSnapFrame = 0;

  const snapSeedToBottom = () => {
    scrollSync.syncSpacer();
    scrollSync.setFollowLive(true);
    setHasUnseenOutput(false);
    scrollSync.setSyncingScroll(true);
    termRef.current?.scrollToBottom();
    scrollSync.scrollInteractionToBottom();
    scrollSync.setSyncingScroll(false);
    scrollSync.refreshFollow();
  };

  // Pin while still hidden, then unhide in place. Never move scrollTop after
  // opacity returns — that is the visible "scrolls all the way down" open.
  const revealSeed = () => {
    clearSeedPendingRevealTimer();
    if (!isActive() || !isSeedPending()) return;
    snapSeedToBottom();
    if (revealSnapFrame) cancelAnimationFrame(revealSnapFrame);
    revealSnapFrame = requestAnimationFrame(() => {
      revealSnapFrame = requestAnimationFrame(() => {
        revealSnapFrame = 0;
        if (!isActive() || !isSeedPending()) return;
        snapSeedToBottom();
        interactionEl.classList.remove("is-seed-pending");
        // Keep scrollOnEraseInDisplay true through seed-pending so a late attach
        // CSI 2 J still pushes the seeded viewport into scrollback. Latch off on
        // the first post-reveal erase (onOutput) or after grace if none is seen.
        armPostRevealEraseGrace();
      });
    });
  };

  // The seed is scrollback only; the tmux attach repaint of the visible pane
  // lands in later frames. Revealing after the first write means watching that
  // repaint scroll a screenful, so hold until output goes quiet.
  const deferSeedReveal = () => {
    if (!isSeedPending()) return;
    if (seedQuietTimer) clearTimeout(seedQuietTimer);
    seedQuietTimer = setTimeout(revealSeed, SEED_REVEAL_QUIET_MS);
    seedCapTimer ??= setTimeout(revealSeed, SEED_REVEAL_MAX_MS);
  };

  // Both timers start at the first write, not at open: a pane that has sent
  // nothing yet is an empty grid, and hiding an empty grid looks identical.
  const beginSeedPending = () => {
    clearSeedPendingRevealTimer();
    clearPostRevealEraseGraceTimer();
    eraseCarry = "";
    interactionEl.classList.add("is-seed-pending");
  };

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
    const fitAddon = terminalAddons?.fitAddon;
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
    // term.resize clears selection; skip all fits while Copy/selection is live
    // (including discrete open/expand settle — a late rAF must not unmount Copy).
    if ((termRef.current?.getSelection() ?? "").length > 0) {
      return;
    }
    if (fitFrame) cancelAnimationFrame(fitFrame);
    fitFrame = requestAnimationFrame(() => {
      fitFrame = 0;
      if (!isActive() || (isKeyboardOpen() && !discreteIntent)) return;
      if ((termRef.current?.getSelection() ?? "").length > 0) return;
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

    const hit = findHttpLinkAtClient(
      termRef.current,
      event.clientX,
      event.clientY,
      hostEl,
    );
    if (hit) {
      setLinkMenu({ url: hit.url, x: event.clientX, y: event.clientY });
      // Keep keyboard closed so fixed menu stays on-band / tappable.
      if (!isKeyboardOpen()) termTextarea()?.blur();
      return;
    }

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
    if (longPressSelected || !termRef.current || !hostEl) return;
    if (selectWordAtClient(termRef.current, clientX, clientY, hostEl)) {
      longPressSelected = true;
    }
  };

  const armSelectionDrag = (touch: Touch, event: TouchEvent) => {
    pendingTapAt = 0;
    setTerminalDoubleTapPending(false);
    selectionDragActive = true;
    setTerminalSelecting(true);
    cancelLongPress();
    clearDirectionalGesture();
    const term = termRef.current;
    if (term && hostEl) {
      selectWordAtClient(term, touch.clientX, touch.clientY, hostEl);
      const cell = clientToBufferCell(term, touch.clientX, touch.clientY, hostEl);
      if (cell) {
        const line = term.buffer.active.getLine(cell.row);
        const bounds = line ? wordBoundsAtCol(line.translateToString(false), cell.col) : null;
        if (bounds) {
          selectionAnchorCol = bounds.start;
          selectionAnchorRow = cell.row;
          selectionAnchorWordEnd = bounds.end;
        } else {
          selectionAnchorCol = cell.col;
          selectionAnchorRow = cell.row;
          selectionAnchorWordEnd = cell.col + 1;
        }
        selectionDragLastCol = cell.col;
        selectionDragLastRow = cell.row;
      }
    }
    if (event.cancelable) event.preventDefault();
  };

  const onTouchStart = (event: TouchEvent) => {
    if (event.touches.length === 1) {
      clearDirectionalGesture();
      const touch = event.touches[0];
      if (
        pendingTapAt > 0 &&
        performance.now() - pendingTapAt <= DOUBLE_TAP_WINDOW_MS &&
        Math.hypot(touch.clientX - pendingTapX, touch.clientY - pendingTapY) <= DOUBLE_TAP_SLOP_PX
      ) {
        armSelectionDrag(touch, event);
      } else {
        if (pendingTapAt > 0) {
          pendingTapAt = 0;
          setTerminalDoubleTapPending(false);
        }
        longPressStartX = touch.clientX;
        longPressStartY = touch.clientY;
        longPressStartedAt = performance.now();
        longPressActive = true;
        longPressSelected = false;
        if (longPressTimer) {
          clearTimeout(longPressTimer);
          longPressTimer = undefined;
        }
      }
    } else {
      pendingTapAt = 0;
      setTerminalDoubleTapPending(false);
      cancelLongPress();
      clearDirectionalGesture();
      if (selectionDragActive) {
        selectionDragActive = false;
        setTerminalSelecting(false);
      }
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
    if (selectionDragActive) {
      if (event.touches.length !== 1) {
        selectionDragActive = false;
        setTerminalSelecting(false);
      } else if (event.cancelable) {
        event.preventDefault();
        const touch = event.touches[0];
        const term = termRef.current;
        if (term && hostEl) {
          const cell = clientToBufferCell(term, touch.clientX, touch.clientY, hostEl);
          if (
            cell &&
            (cell.col !== selectionDragLastCol || cell.row !== selectionDragLastRow)
          ) {
            selectionDragLastCol = cell.col;
            selectionDragLastRow = cell.row;
            selectRangeFromWordAnchor(
              term,
              selectionAnchorCol,
              selectionAnchorWordEnd,
              cell.col,
              cell.row,
              selectionAnchorRow,
            );
          }
        }
      }
    } else if (directionalArmed) {
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
          // Lock page swipe once the hold owns the finger (select or arrows).
          setTerminalSelecting(true);
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
    if (selectionDragActive) {
      selectionDragActive = false;
      setTerminalSelecting(false);
      setTerminalDoubleTapPending(false);
      cancelLongPress();
      clearDirectionalGesture();
    } else {
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
        setTerminalSelecting(false);
      } else if (
        !directionalArmed &&
        longPressActive &&
        !longPressSelected &&
        longPressStartedAt > 0 &&
        performance.now() - longPressStartedAt < DOUBLE_TAP_WINDOW_MS
      ) {
        pendingTapX = longPressStartX;
        pendingTapY = longPressStartY;
        pendingTapAt = performance.now();
        setTerminalDoubleTapPending(true);
        setTerminalSelecting(false);
      } else {
        setTerminalSelecting(false);
        setTerminalDoubleTapPending(false);
      }
      cancelLongPress();
      clearDirectionalGesture();
    }
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
    cursorBlink: false,
    scrollback: terminalScrollbackLines(),
    scrollOnEraseInDisplay: true,
    theme: {
      background: "#161616",
      foreground: "#e6e6e6",
      cursor: "#87afd7",
    },
  });
  terminalAddons = attachTerminalAddons(liveTerm, {
    onLinkActivate: ({ url, clientX, clientY }) => {
      setLinkMenu({ url, x: clientX, y: clientY });
      if (!isKeyboardOpen()) termTextarea()?.blur();
    },
  });
  linkServiceRef.current = terminalAddons.linkService;
  terminalSnapshotRef.current = terminalAddons.snapshot;
  liveTerm.open(hostEl);
  termRef.current = liveTerm;
  onHardenTextarea();

  // xterm leaves plain Space keydown uncancelled (keyCode 32 < 48), so the
  // browser page-scrolls the wrap. Own Space here: one PTY frame, no scroll.
  liveTerm.attachCustomKeyEventHandler((event) => {
    if (event.key === "Backspace" || event.key === "Delete") {
      // Skipping xterm's handling avoids its preventDefault, which is what lets
      // iOS start its hold-to-delete repeat loop. beforeinput sends the DEL.
      if (event.type === "keydown" && !event.isComposing) onSeedTermSentinel();
      return false;
    }
    if (event.key !== " " && event.code !== "Space") return true;
    if (event.ctrlKey || event.altKey || event.metaKey || event.shiftKey) return true;
    if (event.type === "keydown") {
      event.preventDefault();
      sendKey(" ");
    }
    return false;
  });
  syncHostToWrap();
  const viteDev =
    (import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV === true;
  if (viteDev) {
    (hostEl as unknown as { __xterm?: Terminal }).__xterm = liveTerm;
  }
  const selectionDisposable = liveTerm.onSelectionChange(syncCopyOverlay);
  fitLocal();
  scrollSync.syncSpacer();
  scrollSync.refreshFollow();

  const scrollDisposable = liveTerm.onScroll(() => {
    if (isSeedPending()) return;
    scrollSync.onTermScroll();
  });
  const onWrapScroll = () => {
    // Undone caret reveal: never map it onto the PTY viewport.
    if (onRestorePinnedScroll()) return;
    // Do not gate on isSeedPending: wrapper scroll must still flip followLive
    // off so "New output" works if the user (or a test) scrolls during the
    // quiet window. Mid-parse yank is handled by ignoring onTermScroll above.
    scrollSync.onInteractionScroll();
  };
  interactionEl.addEventListener("scroll", onWrapScroll, { passive: true });
  interactionEl.addEventListener("click", onInteractionClick);

  const dataDisposable = liveTerm.onData(onTermData);
  termTextarea()?.addEventListener("beforeinput", onBeforeInput);
  termTextarea()?.addEventListener("input", onInputEvent);
  termTextarea()?.addEventListener("paste", onPaste, { capture: true });

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
        const { sawErase, carry } = detectCsiEraseInDisplay(eraseCarry, text);
        eraseCarry = carry;
        termRef.current?.write(text, () => {
          // Mid-parse xterm onScroll is ignored while seed-pending (above), so
          // followLive stays put across the write. Do not force-follow here —
          // that would re-pin after a wrapper scroll during the quiet window
          // and suppress the "New output" affordance.
          //
          // Latch scrollOnErase off only after seed reveal: releasing at reveal
          // raced the bridge (seed WS frame, then PTY attach ED2) and wiped
          // history when ED2 landed with the option already false.
          if (sawErase && !isSeedPending() && termRef.current?.options.scrollOnEraseInDisplay) {
            latchScrollOnEraseOff();
          }
          scrollSync.applyOutput();
          deferSeedReveal();
        });
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
        if (seeded) {
          beginSeedPending();
          if (isReconnect && termRef.current) {
            scrollSync.setFollowLive(true);
            setHasUnseenOutput(false);
            scrollSync.setSyncingScroll(true);
            termRef.current.reset();
            termRef.current.options.scrollOnEraseInDisplay = true;
            scrollSync.syncSpacer();
            termRef.current.scrollToBottom();
            scrollSync.scrollInteractionToBottom();
            scrollSync.setSyncingScroll(false);
            scrollSync.refreshFollow();
          }
        }
        if (!seeded) {
          cancelSeedPending();
          latchScrollOnEraseOff();
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
    selectionDragActive = false;
    setTerminalSelecting(false);
    setTerminalDoubleTapPending(false);
    clearSeedPendingRevealTimer();
    clearPostRevealEraseGraceTimer();
    if (revealSnapFrame) cancelAnimationFrame(revealSnapFrame);
    revealSnapFrame = 0;
    keyboardClassObserver.disconnect();
    cancelExpandSettle();
    cancelLongPress();
    clearDirectionalGesture();
    cancelScheduledWork();
    refitController?.dispose();
    dataDisposable?.dispose();
    termTextarea()?.removeEventListener("beforeinput", onBeforeInput);
    termTextarea()?.removeEventListener("input", onInputEvent);
    termTextarea()?.removeEventListener("paste", onPaste, { capture: true });
    termTextarea()?.removeEventListener("focus", seedSentinelFromFocus);
    scrollDisposable?.dispose();
    selectionDisposable?.dispose();
    if (copyNoticeTimerRef.current) clearTimeout(copyNoticeTimerRef.current);
    interactionEl.removeEventListener("scroll", onWrapScroll);
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
    cancelSpeechTransport();
    connection?.dispose();
    if (connection && connectionRef.current === connection) {
      connectionRef.current = undefined;
    }
    terminalAddons?.dispose();
    linkServiceRef.current = undefined;
    terminalSnapshotRef.current = undefined;
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
}
