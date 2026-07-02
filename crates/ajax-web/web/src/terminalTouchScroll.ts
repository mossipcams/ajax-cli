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
