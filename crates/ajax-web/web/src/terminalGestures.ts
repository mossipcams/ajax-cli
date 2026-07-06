/**
 * Touch/wheel gesture state machine for the raw terminal host.
 *
 * Wheel/touch scrolling always uses Ajax-owned terminal scrollback: every
 * gesture is captured before renderer layers can handle it and translated into
 * whole-line scroll steps, horizontal pan of the 80-column canvas, or a
 * pinch font-size change — never forwarded into tmux or the foreground app.
 * The pure px→line and momentum math lives in terminalTouchScroll/
 * terminalGeometry; this module owns the event wiring and gesture state.
 */

import { clampPan, pinchFontSize, MIN_FONT_SIZE, MAX_FONT_SIZE } from "./terminalGeometry";
import { flingFrames, wheelNotchesFromDrag } from "./terminalTouchScroll";

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
  /** Whether this mode allows pinch to change terminal font size. */
  pinchEnabled?(): boolean;
  /** Apply a pinch result: set, persist, and refit. */
  setFontSize(px: number): void;
  /** A two-finger pinch just released: flush the pending PTY resize so the
   * rewrap lands with the finger lift instead of after the debounce. */
  pinchEnded?(): void;
}

const TOUCH_SCROLL_THRESHOLD_PX = 6;

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
  let pinchCanZoom = true;

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
    if (event.touches.length === 2) {
      // Own the pinch at touchdown — iOS latches page zoom at the second
      // finger's touchstart, before any touchmove guard can run.
      if (event.cancelable) event.preventDefault();
      touchActive = false;
      pinchStartDistance = touchDistance(event.touches);
      pinchBaseFontSize = host.fontSize();
      pinchMaxFontSize = host.maxFontSize();
      pinchCanZoom = host.pinchEnabled?.() ?? true;
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
  };

  const onTouchMove = (event: TouchEvent) => {
    if (event.touches.length === 2 && pinchStartDistance > 0) {
      // Own the pinch so iOS can't page-zoom; font rounding means the
      // terminal only re-renders when the size crosses a whole pixel.
      if (event.cancelable) event.preventDefault();
      if (!pinchCanZoom) return;
      const next = pinchFontSize(
        pinchBaseFontSize,
        pinchStartDistance,
        touchDistance(event.touches),
        MIN_FONT_SIZE,
        pinchMaxFontSize,
      );
      if (next !== host.fontSize()) host.setFontSize(next);
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
    pinchCanZoom = true;
  };

  const onTouchEnd = () => {
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
    const pinchWasActive = pinchStartDistance > 0;
    if (pinchWasActive) host.pinchEnded?.();
    resetTouchState();
  };

  const onWheel = (event: WheelEvent) => {
    cancelFling();
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
    target.removeEventListener("touchstart", onTouchStart, touchStartOptions);
    target.removeEventListener("touchmove", onTouchMove, touchMoveOptions);
    target.removeEventListener("touchend", onTouchEnd, scrollEndOptions);
    target.removeEventListener("touchcancel", onTouchCancel, scrollEndOptions);
    target.removeEventListener("wheel", onWheel, wheelOptions);
  };
}
