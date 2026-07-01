import { describe, it, expect } from "vitest";
import { wheelNotchesFromDrag } from "./terminalTouchScroll";

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
