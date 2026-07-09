import { describe, it, expect } from "vitest";
import { flingFrames, wheelNotchesFromDrag } from "./terminalGestures";

describe("wheelNotchesFromDrag", () => {
  it("emits no notch until a full cell has been dragged", () => {
    const { notches, remainderPx } = wheelNotchesFromDrag(12, 18);
    expect(notches).toBe(0);
    expect(remainderPx).toBe(12);
  });

  it("emits one notch per cell of drag and carries the remainder", () => {
    const { notches, remainderPx } = wheelNotchesFromDrag(40, 18);
    expect(notches).toBe(2);
    expect(remainderPx).toBe(4);
  });

  it("scrolls back through history on a downward drag (negative accum)", () => {
    const { notches, remainderPx } = wheelNotchesFromDrag(-40, 18);
    expect(notches).toBe(-2);
    expect(remainderPx).toBe(-4);
  });

  it("clamps a fast fling so one swipe cannot flood the PTY", () => {
    const { notches, remainderPx } = wheelNotchesFromDrag(5000, 18, 24);
    expect(notches).toBe(24);
    // The remainder still reflects the full drag so accumulation stays honest.
    expect(remainderPx).toBeCloseTo(5000 - Math.trunc(5000 / 18) * 18);
  });

  it("degrades safely when the cell height is unknown", () => {
    for (const cell of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      const { notches, remainderPx } = wheelNotchesFromDrag(50, cell);
      expect(notches).toBe(0);
      expect(remainderPx).toBe(50);
    }
  });
});

describe("flingFrames", () => {
  const totalLines = (frames: number[]) => frames.reduce((sum, lines) => sum + Math.abs(lines), 0);

  it("yields a finite decaying sequence of line steps for a fast release", () => {
    const frames = flingFrames(2, 18);

    expect(frames.length).toBeGreaterThan(0);
    expect(totalLines(frames)).toBeGreaterThan(0);
    // Decay: the tail of the fling moves fewer lines than the head.
    const half = Math.floor(frames.length / 2);
    expect(totalLines(frames.slice(0, half))).toBeGreaterThanOrEqual(
      totalLines(frames.slice(half)),
    );
    // Positive velocity (finger moved up) only ever scrolls toward newest.
    for (const lines of frames) {
      expect(lines).toBeGreaterThanOrEqual(0);
    }
  });

  it("scrolls back through history on a negative release velocity", () => {
    const frames = flingFrames(-2, 18);

    expect(totalLines(frames)).toBeGreaterThan(0);
    for (const lines of frames) {
      expect(lines).toBeLessThanOrEqual(0);
    }
  });

  it("yields nothing for a slow or stationary release", () => {
    expect(flingFrames(0, 18)).toEqual([]);
    expect(flingFrames(0.01, 18)).toEqual([]);
  });

  it("caps the total distance so one swipe cannot flood the terminal", () => {
    expect(totalLines(flingFrames(500, 18))).toBeLessThanOrEqual(200);
  });

  it("degrades safely for non-finite inputs", () => {
    expect(flingFrames(Number.NaN, 18)).toEqual([]);
    expect(flingFrames(2, 0)).toEqual([]);
    expect(flingFrames(2, Number.NaN)).toEqual([]);
  });
});
