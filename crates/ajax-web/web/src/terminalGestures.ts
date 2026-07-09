/**
 * Touch/wheel gesture state machine for the raw terminal host.
 *
 * Wheel/touch scrolling always uses Ajax-owned terminal scrollback: every
 * gesture is captured before renderer layers can handle it and translated into
 * whole-line scroll steps, horizontal pan of the 80-column canvas, a pinch
 * font-size change, or a long-press copy selection — never forwarded into
 * tmux or the foreground app.
 * The pure px→line and momentum math lives in terminalTouchScroll/
 * terminalGeometry; this module owns the event wiring and gesture state.
 */

import { clampPan, pinchFontSize, pinchActivated, MIN_FONT_SIZE, MAX_FONT_SIZE } from "./terminalGeometry";

export interface TerminalGestureHost {
  /** Scroll the local scrollback; positive = toward newest output. */
  scrollLines(lines: number): void;
  /** Current cell height in px for px→line conversion. */
  cellHeightPx(): number;
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

/**
 * Attach the gesture handlers to `target` (the overflow-hidden terminal
 * host). Returns a detach function that also cancels any running fling.
 */
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

  // Momentum: a fast release keeps scrolling with decaying inertia (the
  // synthetic scroll otherwise stops dead the instant the finger lifts).
  // The frame sequence is precomputed by flingFrames; any new touch or
  // wheel cancels it so the user always wins.
  let flingHandle = 0;
  let flingVelocity = 0; // px per ms, positive = toward newest output
  let lastMoveTime = 0;
  let touchScrolled = false;

  const cancelFling = () => {
    if (flingHandle) {
      cancelAnimationFrame(flingHandle);
      flingHandle = 0;
    }
  };

  const startFling = (frames: number[]) => {
    cancelFling();
    let index = 0;
    const step = () => {
      if (index >= frames.length) {
        flingHandle = 0;
        return;
      }
      const lines = frames[index];
      index += 1;
      if (lines !== 0) host.scrollLines(lines);
      flingHandle = requestAnimationFrame(step);
    };
    flingHandle = requestAnimationFrame(step);
  };

  const onTouchStart = (event: TouchEvent) => {
    cancelFling();
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
    flingVelocity = 0;
    lastMoveTime = performance.now();
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

    // Release-velocity estimate for the momentum fling; low-passed so one
    // jittery event can't spike it.
    const now = performance.now();
    const dtMs = Math.max(1, now - lastMoveTime);
    lastMoveTime = now;
    flingVelocity = 0.8 * (dy / dtMs) + 0.2 * flingVelocity;

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

    // Past the threshold this is a scroll, not a tap, so own the gesture NOW —
    // before a full cell of movement accumulates. iOS Safari latches native
    // momentum scrolling in the first pixels of a drag and can't be cancelled
    // later; preventing default here (rather than only once a whole notch
    // lands) stops that native scroll from racing our scrollLines() and stops
    // iOS from synthesizing the click that would pop the keyboard.
    if (event.cancelable) event.preventDefault();

    // Horizontal component pans the 80-col canvas within the host; the host
    // is overflow:hidden so only this handler ever moves it.
    if (touchAccumXPx !== 0) {
      target.scrollLeft = clampPan(
        target.scrollLeft + touchAccumXPx,
        target.scrollWidth,
        target.clientWidth,
      );
      touchAccumXPx = 0;
    }

    const { notches, remainderPx } = wheelNotchesFromDrag(touchAccumPx, host.cellHeightPx());
    touchAccumPx = remainderPx;
    if (notches === 0) return;
    host.scrollLines(notches);
  };

  const resetTouchState = () => {
    flingVelocity = 0;
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
    // Only a gesture that actually scrolled may fling; a tap with a few
    // pixels of jitter must stay a tap.
    if (touchActive && touchScrolled) {
      const frames = flingFrames(flingVelocity, host.cellHeightPx());
      if (frames.length) startFling(frames);
    }
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

  const onWheel = (event: WheelEvent) => {
    cancelFling();
    cancelLongPress();
    const lineDelta =
      event.deltaMode === WheelEvent.DOM_DELTA_PIXEL
        ? Math.trunc(event.deltaY / host.cellHeightPx())
        : Math.trunc(event.deltaY);
    if (lineDelta === 0) return;

    if (event.cancelable) event.preventDefault();
    host.scrollLines(lineDelta);
  };

  // Capture phase so renderer layers can never swallow the gesture;
  // touchmove/wheel are non-passive because owning the gesture requires
  // preventDefault (see the iOS notes above).
  const touchStartOptions: AddEventListenerOptions = { passive: false, capture: true };
  const touchMoveOptions: AddEventListenerOptions = { passive: false, capture: true };
  const scrollEndOptions: AddEventListenerOptions = { passive: true, capture: true };
  const wheelOptions: AddEventListenerOptions = { passive: false, capture: true };

  target.addEventListener("touchstart", onTouchStart, touchStartOptions);
  target.addEventListener("touchmove", onTouchMove, touchMoveOptions);
  target.addEventListener("touchend", onTouchEnd, scrollEndOptions);
  target.addEventListener("touchcancel", onTouchCancel, scrollEndOptions);
  target.addEventListener("wheel", onWheel, wheelOptions);

  return () => {
    cancelFling();
    cancelLongPress();
    target.removeEventListener("touchstart", onTouchStart, touchStartOptions);
    target.removeEventListener("touchmove", onTouchMove, touchMoveOptions);
    target.removeEventListener("touchend", onTouchEnd, scrollEndOptions);
    target.removeEventListener("touchcancel", onTouchCancel, scrollEndOptions);
    target.removeEventListener("wheel", onWheel, wheelOptions);
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


// --- touch scroll (folded from terminalTouchScroll.ts) ---
/**
 * Touch-drag → scroll-line math for the terminal's synthetic scrolling.
 *
 * Browser terminal renderers expose their own layered DOM/canvas surfaces, so
 * native touch scrolling is not reliable. terminalGestures owns scrolling instead,
 * translating drags into local `term.scrollLines()` steps; this module holds
 * the pure math: accumulated drag pixels become whole line "notches" while
 * the leftover sub-cell pixels carry forward so slow drags scroll smoothly.
 */

export interface WheelNotches {
  /** Whole line steps to scroll; positive scrolls toward newest output. */
  notches: number;
  /** Leftover sub-cell pixels to carry into the next move. */
  remainderPx: number;
}

/**
 * Split `accumulatedPx` of vertical drag into whole line notches of
 * `cellPx` each, returning the sub-cell remainder to accumulate next time.
 * `maxNotches` bounds a single move's local scrollback jump. (Scroll never
 * reaches the PTY — gestures drive Ajax-owned scrollback only.)
 */
export function wheelNotchesFromDrag(
  accumulatedPx: number,
  cellPx: number,
  maxNotches = 24,
): WheelNotches {
  if (!Number.isFinite(cellPx) || cellPx <= 0) {
    return { notches: 0, remainderPx: accumulatedPx };
  }
  const whole = Math.trunc(accumulatedPx / cellPx);
  const remainderPx = accumulatedPx - whole * cellPx;
  const notches = Math.max(-maxNotches, Math.min(maxNotches, whole));
  return { notches, remainderPx };
}

/**
 * Convert a touch-release velocity into a momentum "fling": a finite,
 * pre-computed sequence of per-animation-frame line steps that decays to a
 * stop. Native momentum scrolling is disabled on the terminal (it desyncs
 * from `scrollLines`), so this provides the inertia a drag-only scroll lacks.
 *
 * `velocityPxPerMs` is positive when the finger moved up (scroll toward the
 * newest output). Sub-threshold or non-finite velocities yield no frames, and
 * the total distance is capped so one hard swipe cannot flood the terminal.
 */
export function flingFrames(
  velocityPxPerMs: number,
  cellPx: number,
  decayPerFrame = 0.92,
  minVelocityPxPerMs = 0.05,
  maxTotalLines = 200,
): number[] {
  if (
    !Number.isFinite(velocityPxPerMs) ||
    !Number.isFinite(cellPx) ||
    cellPx <= 0 ||
    Math.abs(velocityPxPerMs) < minVelocityPxPerMs
  ) {
    return [];
  }

  const FRAME_MS = 16;
  const frames: number[] = [];
  let velocity = velocityPxPerMs;
  let carryPx = 0;
  let total = 0;

  while (Math.abs(velocity) >= minVelocityPxPerMs && total < maxTotalLines) {
    carryPx += velocity * FRAME_MS;
    let lines = Math.trunc(carryPx / cellPx);
    if (lines !== 0) {
      // Never exceed the cap even mid-frame.
      const room = maxTotalLines - total;
      lines = Math.max(-room, Math.min(room, lines));
      carryPx -= lines * cellPx;
      total += Math.abs(lines);
    }
    frames.push(lines);
    velocity *= decayPerFrame;
  }

  return frames;
}
