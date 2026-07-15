/**
 * Touch gesture state machine for the raw terminal host.
 *
 * Vertical scrolling is native (the host is overflow-y:auto with a scroll
 * spacer); this module owns horizontal pan of the 80-column canvas, pinch
 * font-size change, long-press copy selection, and early touch focus — never
 * forwarded into tmux or the foreground app.
 */

import { clampPan, pinchFontSize, pinchActivated, MIN_FONT_SIZE, MAX_FONT_SIZE } from "./terminalGeometry";

export interface TerminalGestureHost {
  /** Current terminal font size (pinch baseline). */
  fontSize(): number;
  /** Largest font a pinch may reach: the size at which the column floor
   * still fits the host width, so zooming never pushes text off-screen. */
  maxFontSize(): number;
  /** Apply a pinch result: set, persist, and refit. */
  setFontSize(px: number): void;
  /** A two-finger pinch just released: flush the pending PTY resize so the
   * rewrap lands with the finger lift instead of after the debounce. */
  pinchEnded?(): void;
  /** A long-press anchored a text selection at these client coordinates. */
  beginSelection?(clientX: number, clientY: number): void;
  /** The selecting finger dragged to these client coordinates. */
  extendSelection?(clientX: number, clientY: number): void;
  /** The selecting finger lifted; cancelled means the system stole the
   * gesture (or a second finger landed) and the selection must be dropped
   * instead of copied. */
  endSelection?(cancelled: boolean): void;
  /** A single finger just touched down. Hosts use this to focus the hidden
   * textarea early so iOS can attach native Paste before long-press fires. */
  touchBegan?(): void;
}

const TOUCH_SCROLL_THRESHOLD_PX = 6;
// A two-finger touch must change the finger distance by this many px before it
// counts as a zoom, so an incidental graze can't rewrap the terminal.
const PINCH_ACTIVATION_PX = 12;
// iOS's own text long-press engages around 500ms; matching it makes the
// synthesized selection feel native instead of laggy or hair-triggered.
const LONG_PRESS_MS = 500;

/** Attach gesture handlers to `target` (the terminal host). */
export function attachTerminalGestures(
  target: HTMLElement,
  host: TerminalGestureHost,
): () => void {
  let touchActive = false;
  let touchLastY = 0;
  let touchAccumPx = 0;
  let touchLastX = 0;
  let touchAccumXPx = 0;

  // Two-finger pinch adjusts the font size (the legibility ↔ visible-columns
  // lever now that the PTY keeps an 80-column floor). The gesture scales
  // from the size it started at, so a slow pinch can't compound drift. The
  // ceiling is captured at pinch start: the host width can't change mid-
  // gesture, and one measurement per gesture beats one per touchmove.
  let pinchStartDistance = 0;
  let pinchBaseFontSize = 0;
  let pinchMaxFontSize = MAX_FONT_SIZE;
  let pinchEngaged = false;

  // Long-press → drag → lift = select → extend → copy. The timer arms on a
  // single stationary finger; any scroll, pinch, wheel, or lift disarms it.
  let longPressTimer: ReturnType<typeof setTimeout> | undefined;
  let selecting = false;

  const cancelLongPress = () => {
    if (longPressTimer) {
      clearTimeout(longPressTimer);
      longPressTimer = undefined;
    }
  };

  const touchDistance = (touches: TouchList): number =>
    Math.hypot(touches[0].clientX - touches[1].clientX, touches[0].clientY - touches[1].clientY);

  let touchScrolled = false;

  const onTouchStart = (event: TouchEvent) => {
    cancelLongPress();
    if (event.touches.length === 2) {
      // Own the pinch at touchdown — iOS latches page zoom at the second
      // finger's touchstart, before any touchmove guard can run.
      if (event.cancelable) event.preventDefault();
      // A second finger during a live selection turns the gesture into a
      // pinch; the half-made selection must not be copied.
      if (selecting) {
        selecting = false;
        host.endSelection?.(true);
      }
      touchActive = false;
      pinchEngaged = false;
      pinchStartDistance = touchDistance(event.touches);
      pinchBaseFontSize = host.fontSize();
      pinchMaxFontSize = host.maxFontSize();
      return;
    }
    pinchStartDistance = 0;
    if (event.touches.length !== 1) {
      touchActive = false;
      return;
    }
    touchActive = true;
    touchScrolled = false;
    touchAccumPx = 0;
    touchAccumXPx = 0;
    touchLastY = event.touches[0].clientY;
    touchLastX = event.touches[0].clientX;
    if (host.beginSelection) {
      longPressTimer = setTimeout(() => {
        longPressTimer = undefined;
        // Only a finger that is still down and hasn't scrolled anchors a
        // selection; touchLastX/Y track sub-threshold jitter so the anchor
        // lands where the finger actually rests.
        if (!touchActive || touchScrolled) return;
        touchActive = false;
        selecting = true;
        host.beginSelection?.(touchLastX, touchLastY);
      }, LONG_PRESS_MS);
    }
    // Focus before the long-press timer so iOS can target native Paste at an
    // editable field. Do not preventDefault here — that would kill Paste.
    host.touchBegan?.();
  };

  const onTouchMove = (event: TouchEvent) => {
    if (selecting && event.touches.length === 1) {
      // The drag now moves the selection focus, never the scrollback; owning
      // the event also keeps iOS from starting a native pan mid-selection.
      if (event.cancelable) event.preventDefault();
      const touch = event.touches[0];
      touchLastX = touch.clientX;
      touchLastY = touch.clientY;
      host.extendSelection?.(touch.clientX, touch.clientY);
      return;
    }
    if (event.touches.length === 2 && pinchStartDistance > 0) {
      // Own the pinch so iOS can't page-zoom; font rounding means the
      // terminal only re-renders when the size crosses a whole pixel.
      if (event.cancelable) event.preventDefault();
      const distance = touchDistance(event.touches);
      if (!pinchEngaged && pinchActivated(pinchStartDistance, distance, PINCH_ACTIVATION_PX)) {
        pinchEngaged = true;
      }
      if (pinchEngaged) {
        const next = pinchFontSize(
          pinchBaseFontSize,
          pinchStartDistance,
          distance,
          MIN_FONT_SIZE,
          pinchMaxFontSize,
        );
        if (next !== host.fontSize()) host.setFontSize(next);
      }
      return;
    }
    if (!touchActive || event.touches.length !== 1) return;
    const touch = event.touches[0];
    const dy = touchLastY - touch.clientY;
    touchAccumPx += dy;
    touchAccumXPx += touchLastX - touch.clientX;
    touchLastY = touch.clientY;
    touchLastX = touch.clientX;

    if (
      Math.abs(touchAccumPx) < TOUCH_SCROLL_THRESHOLD_PX &&
      Math.abs(touchAccumXPx) < TOUCH_SCROLL_THRESHOLD_PX
    ) {
      return;
    }
    touchScrolled = true;
    // Past the threshold the finger is scrolling, not resting: a long-press
    // can no longer begin.
    cancelLongPress();

    const horizontalDominant = Math.abs(touchAccumXPx) > Math.abs(touchAccumPx);
    // Own horizontal pans only; vertical drags fall through to native scroll.
    if (horizontalDominant && event.cancelable) event.preventDefault();

    if (horizontalDominant && touchAccumXPx !== 0) {
      target.scrollLeft = clampPan(
        target.scrollLeft + touchAccumXPx,
        target.scrollWidth,
        target.clientWidth,
      );
      touchAccumXPx = 0;
    }
  };

  const resetTouchState = () => {
    touchActive = false;
    touchAccumPx = 0;
    touchAccumXPx = 0;
    pinchStartDistance = 0;
    pinchEngaged = false;
  };

  const onTouchEnd = (event: TouchEvent) => {
    cancelLongPress();
    if (selecting) {
      selecting = false;
      host.endSelection?.(false);
      // ghostty-web's canvas focuses its hidden textarea on every touchend,
      // which pops the iOS keyboard; a copy gesture must stay a copy.
      event.stopPropagation();
      resetTouchState();
      return;
    }
    const pinchWasActive = pinchStartDistance > 0;
    if (pinchWasActive) host.pinchEnded?.();
    resetTouchState();
  };

  // touchcancel means the system stole the gesture (e.g. an incoming call
  // sheet); momentum from a stolen gesture would feel haunted, so reset
  // without flinging.
  const onTouchCancel = () => {
    cancelLongPress();
    if (selecting) {
      selecting = false;
      host.endSelection?.(true);
    }
    const pinchWasActive = pinchStartDistance > 0;
    if (pinchWasActive) host.pinchEnded?.();
    resetTouchState();
  };

  // Capture phase so renderer layers can never swallow horizontal pan or pinch.
  const touchStartOptions: AddEventListenerOptions = { passive: false, capture: true };
  const touchMoveOptions: AddEventListenerOptions = { passive: false, capture: true };
  const scrollEndOptions: AddEventListenerOptions = { passive: true, capture: true };

  target.addEventListener("touchstart", onTouchStart, touchStartOptions);
  target.addEventListener("touchmove", onTouchMove, touchMoveOptions);
  target.addEventListener("touchend", onTouchEnd, scrollEndOptions);
  target.addEventListener("touchcancel", onTouchCancel, scrollEndOptions);

  return () => {
    cancelLongPress();
    target.removeEventListener("touchstart", onTouchStart, touchStartOptions);
    target.removeEventListener("touchmove", onTouchMove, touchMoveOptions);
    target.removeEventListener("touchend", onTouchEnd, scrollEndOptions);
    target.removeEventListener("touchcancel", onTouchCancel, scrollEndOptions);
  };
}


// --- selection (folded from terminalSelection.ts) ---
/**
 * Pure math for touch (long-press) text selection on the canvas terminal.
 *
 * ghostty-web renders to a <canvas>, so native browser selection can never
 * work; Ajax synthesizes it from touch gestures instead. These helpers map
 * touch points to terminal cells and order a dragged range so the gesture
 * wiring in terminalGestures and the terminal plumbing in TerminalRawView
 * stay thin and the math stays unit-testable.
 */

export interface CellPoint {
  col: number;
  row: number;
}

/**
 * Map a point (px, relative to the rendered grid's top-left) to the cell it
 * falls in, clamped into the grid so a drag past any edge selects to that
 * edge instead of vanishing. Returns undefined when the grid has no
 * measurable size (pre-layout, jsdom).
 */
export function cellAtPoint(
  xPx: number,
  yPx: number,
  gridWidthPx: number,
  gridHeightPx: number,
  cols: number,
  rows: number,
): CellPoint | undefined {
  if (
    !Number.isFinite(xPx) ||
    !Number.isFinite(yPx) ||
    !Number.isFinite(gridWidthPx) ||
    !Number.isFinite(gridHeightPx) ||
    gridWidthPx <= 0 ||
    gridHeightPx <= 0 ||
    !Number.isInteger(cols) ||
    !Number.isInteger(rows) ||
    cols <= 0 ||
    rows <= 0
  ) {
    return undefined;
  }
  const col = Math.min(cols - 1, Math.max(0, Math.floor((xPx / gridWidthPx) * cols)));
  const row = Math.min(rows - 1, Math.max(0, Math.floor((yPx / gridHeightPx) * rows)));
  return { col, row };
}

/**
 * Order two selection endpoints into reading order (top-left first), so a
 * drag upward/backward selects the same range as the forward drag.
 */
export function orderedSelection(
  a: CellPoint,
  b: CellPoint,
): { start: CellPoint; end: CellPoint } {
  const backward = a.row > b.row || (a.row === b.row && a.col > b.col);
  return backward ? { start: b, end: a } : { start: a, end: b };
}
