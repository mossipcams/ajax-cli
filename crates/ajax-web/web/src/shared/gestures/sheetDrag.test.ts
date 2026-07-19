import { describe, it, expect } from "vitest";
import { sheetStart, sheetMove, sheetEnd, SHEET_DISMISS_THRESHOLD } from "./sheetDrag";

describe("sheet drag-to-dismiss gesture", () => {
  it("tracks downward drag distance", () => {
    expect(sheetMove(sheetStart(), 40).offset).toBe(40);
  });

  it("clamps upward drag to zero", () => {
    expect(sheetMove(sheetStart(), -60).offset).toBe(0);
  });

  it("dismisses when released past the threshold", () => {
    const dragged = sheetMove(sheetStart(), SHEET_DISMISS_THRESHOLD + 10);
    expect(sheetEnd(dragged).dismiss).toBe(true);
  });

  it("springs back when released before the threshold", () => {
    const dragged = sheetMove(sheetStart(), SHEET_DISMISS_THRESHOLD - 10);
    expect(sheetEnd(dragged)).toEqual({ dismiss: false, offset: 0 });
  });
});
