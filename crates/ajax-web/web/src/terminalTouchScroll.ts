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
