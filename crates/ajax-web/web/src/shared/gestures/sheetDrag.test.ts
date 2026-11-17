import { describe, it, expect } from "vitest";
import {
  sheetDismisses,
  sheetOffset,
  sheetReleaseVelocity,
  SHEET_DISMISS_THRESHOLD,
  SHEET_FLICK_VELOCITY,
} from "./sheetDrag";

describe("sheet drag-to-dismiss gesture", () => {
  it("tracks downward drag distance one to one", () => {
    expect(sheetOffset(40)).toBe(40);
  });

  it("rubber-bands upward drag instead of clamping to zero (#1616)", () => {
    const up = sheetOffset(-100);
    expect(up).toBeLessThan(0);
    expect(Math.abs(up)).toBeGreaterThan(0);
    expect(Math.abs(up)).toBeLessThan(100);
  });

  it("resists upward drag more the further it travels (#1616)", () => {
    expect(Math.abs(sheetOffset(-200))).toBeLessThan(2 * Math.abs(sheetOffset(-100)));
  });

  it("dismisses when released past the distance threshold", () => {
    expect(sheetDismisses(SHEET_DISMISS_THRESHOLD, 0)).toBe(true);
  });

  it("springs back when released before the threshold at rest speed", () => {
    expect(sheetDismisses(SHEET_DISMISS_THRESHOLD - 10, 0.1)).toBe(false);
  });

  it("dismisses a short fast downward flick below the distance threshold (#1616)", () => {
    expect(sheetDismisses(30, SHEET_FLICK_VELOCITY)).toBe(true);
  });

  it("does not dismiss an upward release even when it is fast", () => {
    expect(sheetDismisses(0, -2)).toBe(false);
  });

  it("measures release velocity from the last 100ms of samples", () => {
    const samples = [
      { t: 0, y: 0 },
      { t: 1300, y: 0 },
      { t: 1350, y: 15 },
      { t: 1400, y: 30 },
    ];
    expect(sheetReleaseVelocity(samples, 1400)).toBeCloseTo(0.3);
  });
});
