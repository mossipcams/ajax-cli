import { describe, it, expect, beforeEach } from "vitest";
import {
  flooredCols,
  clampPan,
  pinchFontSize,
  fitCapFontSize,
  persistedGeometryMode,
  persistGeometryMode,
} from "./terminalGeometry";

describe("flooredCols", () => {
  it("raises a narrow proposal to the minimum column count", () => {
    expect(flooredCols(55, 80)).toBe(80);
  });

  it("keeps a wide proposal untouched", () => {
    expect(flooredCols(120, 80)).toBe(120);
  });

  it("keeps an exact-fit proposal untouched", () => {
    expect(flooredCols(80, 80)).toBe(80);
  });

  it("falls back to the floor when the proposal is undefined", () => {
    expect(flooredCols(undefined, 80)).toBe(80);
  });

  it("falls back to the floor when the proposal is not finite or non-positive", () => {
    expect(flooredCols(Number.NaN, 80)).toBe(80);
    expect(flooredCols(0, 80)).toBe(80);
    expect(flooredCols(-3, 80)).toBe(80);
  });
});

describe("clampPan", () => {
  it("passes through a pan inside the scrollable range", () => {
    expect(clampPan(60, 480, 338)).toBe(60);
  });

  it("clamps a pan past the right edge to the maximum", () => {
    expect(clampPan(500, 480, 338)).toBe(142);
  });

  it("clamps a negative pan to zero", () => {
    expect(clampPan(-20, 480, 338)).toBe(0);
  });

  it("returns zero when the content fits inside the viewport", () => {
    expect(clampPan(50, 300, 338)).toBe(0);
  });

  it("returns zero for non-finite inputs", () => {
    expect(clampPan(Number.NaN, 480, 338)).toBe(0);
    expect(clampPan(60, Number.NaN, 338)).toBe(0);
    expect(clampPan(60, 480, Number.NaN)).toBe(0);
  });
});

describe("fitCapFontSize", () => {
  it("returns the largest font at which the column floor still fits", () => {
    // 48 columns fit at 13px, so the same width holds 80 columns at
    // floor(13 * 48 / 80) = 7px.
    expect(fitCapFontSize(13, 48, 80)).toBe(7);
  });

  it("leaves headroom untouched when the floor already fits with room", () => {
    expect(fitCapFontSize(13, 100, 80)).toBe(16);
  });

  it("clamps the cap to the maximum font size", () => {
    expect(fitCapFontSize(13, 200, 80)).toBe(20);
  });

  it("clamps the cap to the minimum font size when even that overflows", () => {
    expect(fitCapFontSize(13, 30, 80)).toBe(7);
  });

  it("returns the maximum (no constraint) for invalid column proposals", () => {
    expect(fitCapFontSize(13, undefined, 80)).toBe(20);
    expect(fitCapFontSize(13, Number.NaN, 80)).toBe(20);
    expect(fitCapFontSize(13, 0, 80)).toBe(20);
    expect(fitCapFontSize(13, -5, 80)).toBe(20);
  });

  it("returns the maximum (no constraint) for invalid font sizes", () => {
    expect(fitCapFontSize(0, 48, 80)).toBe(20);
    expect(fitCapFontSize(Number.NaN, 48, 80)).toBe(20);
  });

  it("honors custom clamp bounds", () => {
    expect(fitCapFontSize(13, 100, 80, 8, 14)).toBe(14);
    expect(fitCapFontSize(13, 30, 80, 8, 14)).toBe(8);
  });
});

describe("pinchFontSize", () => {
  it("scales the base font by the pinch distance ratio", () => {
    expect(pinchFontSize(10, 100, 150)).toBe(15);
  });

  it("shrinks the font when the fingers move together", () => {
    expect(pinchFontSize(10, 100, 80)).toBe(8);
  });

  it("clamps to the minimum font size", () => {
    expect(pinchFontSize(10, 100, 10)).toBe(7);
  });

  it("clamps to the maximum font size", () => {
    expect(pinchFontSize(10, 100, 900)).toBe(20);
  });

  it("rounds to a whole pixel size", () => {
    expect(pinchFontSize(10, 100, 112)).toBe(11);
  });

  it("returns the base for zero or non-finite distances", () => {
    expect(pinchFontSize(10, 0, 150)).toBe(10);
    expect(pinchFontSize(10, Number.NaN, 150)).toBe(10);
    expect(pinchFontSize(10, 100, Number.NaN)).toBe(10);
  });

  it("honors custom clamp bounds", () => {
    expect(pinchFontSize(10, 100, 300, 7, 16)).toBe(16);
  });
});

describe("geometry mode persistence", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("round-trips a persisted wide mode", () => {
    persistGeometryMode("wide");
    expect(persistedGeometryMode()).toBe("wide");
  });

  it("returns undefined for stored garbage values", () => {
    window.localStorage.setItem("ajax.terminal.geometryMode", "huge");
    expect(persistedGeometryMode()).toBeUndefined();
    window.localStorage.setItem("ajax.terminal.geometryMode", "");
    expect(persistedGeometryMode()).toBeUndefined();
  });

  it("returns undefined when the key is absent", () => {
    expect(persistedGeometryMode()).toBeUndefined();
  });
});
