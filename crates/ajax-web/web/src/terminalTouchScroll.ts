/**
 * Touch-drag → wheel-notch math for the mobile terminal.
 *
 * xterm 6 ships VS Code's touch-gesture code but never wires it up, and its
 * `.xterm-screen` overlays the scrollable `.xterm-viewport`, so native touch
 * scrolling never fires — the terminal is completely unscrollable on touch
 * devices. TerminalPanel bridges the gap by turning vertical drags into
 * synthetic wheel events; this helper converts accumulated drag pixels into a
 * whole number of wheel "notches" (one line each) while carrying the leftover
 * sub-cell pixels forward so slow drags still scroll smoothly.
 */

export interface WheelNotches {
  /** Whole wheel notches to emit; positive scrolls toward newest output. */
  notches: number;
  /** Leftover sub-cell pixels to carry into the next move. */
  remainderPx: number;
}

/**
 * Split `accumulatedPx` of vertical drag into whole wheel notches of
 * `cellPx` each, returning the sub-cell remainder to accumulate next time.
 * `maxNotches` clamps a single fling so one fast swipe can't flood the PTY.
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
