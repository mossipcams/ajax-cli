import { describe, it, expect, vi, afterEach } from "vitest";
import {
  flooredCols,
  logicalCols,
  scaledLogicalRows,
  fitScale,
  clampPan,
  pinchFontSize,
  pinchActivated,
  fitCapFontSize,
  fitFontSize,
  MIN_FONT_SIZE,
  MOBILE_SCROLLBACK_LINES,
  DESKTOP_SCROLLBACK_LINES,
  terminalScrollbackLines,
} from "./terminalGeometry";

const MOBILE_MEDIA_QUERY =
  "(max-width: 767px), (pointer: coarse) and (max-height: 500px)";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("terminal scrollback limits", () => {
  it("uses 2000 lines on mobile and 10000 on desktop", () => {
    expect(MOBILE_SCROLLBACK_LINES).toBe(2000);
    expect(DESKTOP_SCROLLBACK_LINES).toBe(10000);
  });

  it("selects mobile scrollback when the TaskDetail mobile media heuristic matches", () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn((query: string) => ({
        matches: query === MOBILE_MEDIA_QUERY,
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    );

    expect(terminalScrollbackLines()).toBe(MOBILE_SCROLLBACK_LINES);
    expect(window.matchMedia).toHaveBeenCalledWith(MOBILE_MEDIA_QUERY);
  });

  it("selects desktop scrollback when the mobile media heuristic does not match", () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn((query: string) => ({
        matches: false,
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    );

    expect(terminalScrollbackLines()).toBe(DESKTOP_SCROLLBACK_LINES);
  });
});

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

describe("logicalCols", () => {
  it("floors phone hostFit up to MIN_TERMINAL_COLS", () => {
    expect(logicalCols(43)).toBe(80);
  });

  it("keeps a wide hostFit proposal untouched", () => {
    expect(logicalCols(120)).toBe(120);
  });

  it("falls back to MIN_TERMINAL_COLS when hostFit is absent or invalid", () => {
    expect(logicalCols(undefined)).toBe(80);
    expect(logicalCols(Number.NaN)).toBe(80);
    expect(logicalCols(0)).toBe(80);
  });
});

describe("scaledLogicalRows", () => {
  it("raises host-fit rows when scale is below 1", () => {
    expect(scaledLogicalRows(30, 0.609375)).toBe(50);
    expect(scaledLogicalRows(30, 1)).toBe(30);
    expect(scaledLogicalRows(30, 0)).toBe(30);
    expect(scaledLogicalRows(undefined, 0.5)).toBe(24);
  });
});

describe("fitScale", () => {
  it("is below 1 when logical canvas is wider than host", () => {
    expect(fitScale(390, 80, 9)).toBeLessThan(1);
  });

  it("is 1 when the logical canvas fits the host", () => {
    expect(fitScale(1200, 80, 9)).toBe(1);
  });

  it("returns 1 for invalid measurements", () => {
    expect(fitScale(0, 80, 9)).toBe(1);
    expect(fitScale(390, 0, 9)).toBe(1);
    expect(fitScale(390, 80, 0)).toBe(1);
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

describe("fitFontSize", () => {
  it("returns the font at which the column floor exactly fills the host (fit-font)", () => {
    expect(fitFontSize(384, 80, 8, 13)).toBe(7.75);
  });

  it("quantizes to 0.25px steps (fit-font)", () => {
    expect(fitFontSize(640, 80, 8, 13)).toBe(13);
  });

  it("may return sizes below the pinch minimum (fit-font)", () => {
    expect(fitFontSize(160, 80, 8, 13)).toBeLessThan(MIN_FONT_SIZE);
  });

  it("returns undefined for invalid measurements (fit-font)", () => {
    expect(fitFontSize(0, 80, 8, 13)).toBeUndefined();
    expect(fitFontSize(384, 0, 8, 13)).toBeUndefined();
    expect(fitFontSize(384, 80, 0, 13)).toBeUndefined();
    expect(fitFontSize(384, 80, 8, 0)).toBeUndefined();
    expect(fitFontSize(Number.NaN, 80, 8, 13)).toBeUndefined();
  });
});

describe("pinchActivated", () => {
  it("is inactive below the threshold", () => {
    expect(pinchActivated(100, 108, 12)).toBe(false);
  });

  it("activates when spreading past the threshold", () => {
    expect(pinchActivated(100, 113, 12)).toBe(true);
  });

  it("activates when pinching in past the threshold", () => {
    expect(pinchActivated(100, 87, 12)).toBe(true);
  });

  it("activates exactly at the threshold", () => {
    expect(pinchActivated(100, 112, 12)).toBe(true);
  });

  it("is inactive for non-finite or non-positive distances", () => {
    expect(pinchActivated(0, 120, 12)).toBe(false);
    expect(pinchActivated(100, Number.NaN, 12)).toBe(false);
    expect(pinchActivated(Number.NaN, 120, 12)).toBe(false);
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
