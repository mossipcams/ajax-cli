import { describe, it, expect, vi, afterEach } from "vitest";
import {
  MIN_TERMINAL_COLS,
  DEFAULT_FONT_SIZE,
  MIN_FONT_SIZE,
  MAX_FONT_SIZE,
  FONT_STORAGE_KEY,
  MOBILE_SCROLLBACK_LINES,
  DESKTOP_SCROLLBACK_LINES,
  parsePersistedFontSize,
  computeTerminalGeometry,
  terminalScrollbackLines,
} from "./terminalGeometry";

describe("terminalGeometry constants", () => {
  it("pins the column floor and persisted-font limits to the values TaskTerminal relies on", () => {
    expect(MIN_TERMINAL_COLS).toBe(80);
    expect(DEFAULT_FONT_SIZE).toBe(13);
    expect(MIN_FONT_SIZE).toBe(7);
    expect(MAX_FONT_SIZE).toBe(20);
    expect(FONT_STORAGE_KEY).toBe("ajax.terminal.fontSize");
  });
});

describe("terminal scrollback limits", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("pins mobile and desktop scrollback caps", () => {
    expect(MOBILE_SCROLLBACK_LINES).toBe(2000);
    expect(DESKTOP_SCROLLBACK_LINES).toBe(10000);
  });

  it("returns mobile scrollback when the mobile media query matches", () => {
    vi.stubGlobal("window", {
      matchMedia: vi.fn().mockReturnValue({ matches: true }),
    });
    expect(terminalScrollbackLines()).toBe(MOBILE_SCROLLBACK_LINES);
  });

  it("returns desktop scrollback when the mobile media query does not match", () => {
    vi.stubGlobal("window", {
      matchMedia: vi.fn().mockReturnValue({ matches: false }),
    });
    expect(terminalScrollbackLines()).toBe(DESKTOP_SCROLLBACK_LINES);
  });
});

describe("parsePersistedFontSize", () => {
  it("returns DEFAULT_FONT_SIZE when the raw value is missing or not a finite number", () => {
    expect(parsePersistedFontSize(null)).toBe(DEFAULT_FONT_SIZE);
    expect(parsePersistedFontSize("")).toBe(DEFAULT_FONT_SIZE);
    expect(parsePersistedFontSize("abc")).toBe(DEFAULT_FONT_SIZE);
    expect(parsePersistedFontSize("NaN")).toBe(DEFAULT_FONT_SIZE);
    expect(parsePersistedFontSize("Infinity")).toBe(DEFAULT_FONT_SIZE);
  });

  it("returns DEFAULT_FONT_SIZE for values outside [MIN_FONT_SIZE, MAX_FONT_SIZE]", () => {
    expect(parsePersistedFontSize("6.9")).toBe(DEFAULT_FONT_SIZE);
    expect(parsePersistedFontSize("20.1")).toBe(DEFAULT_FONT_SIZE);
  });

  it("passes through in-bounds integer and fractional values untouched", () => {
    expect(parsePersistedFontSize("7")).toBe(7);
    expect(parsePersistedFontSize("20")).toBe(20);
    expect(parsePersistedFontSize("13.5")).toBe(13.5);
  });
});

describe("computeTerminalGeometry", () => {
  it("returns null when the proposed dimensions are non-integer, non-positive, or non-finite", () => {
    expect(
      computeTerminalGeometry({
        proposedCols: 12.5,
        proposedRows: 24,
        hostWidth: 390,
        hostHeight: 400,
        cellWidth: 8,
        cellHeight: 17,
        fontSize: 13,
      }),
    ).toBeNull();
    expect(
      computeTerminalGeometry({
        proposedCols: 0,
        proposedRows: 24,
        hostWidth: 390,
        hostHeight: 400,
        cellWidth: 8,
        cellHeight: 17,
        fontSize: 13,
      }),
    ).toBeNull();
    expect(
      computeTerminalGeometry({
        proposedCols: 80,
        proposedRows: -1,
        hostWidth: 390,
        hostHeight: 400,
        cellWidth: 8,
        cellHeight: 17,
        fontSize: 13,
      }),
    ).toBeNull();
    expect(
      computeTerminalGeometry({
        proposedCols: Number.NaN,
        proposedRows: 24,
        hostWidth: 390,
        hostHeight: 400,
        cellWidth: 8,
        cellHeight: 17,
        fontSize: 13,
      }),
    ).toBeNull();
  });

  it("returns null when the host or cell measurements are zero", () => {
    const base = {
      proposedCols: 40,
      proposedRows: 12,
      hostWidth: 390,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 13,
    };
    expect(computeTerminalGeometry({ ...base, hostWidth: 0 })).toBeNull();
    expect(computeTerminalGeometry({ ...base, hostHeight: 0 })).toBeNull();
    expect(computeTerminalGeometry({ ...base, cellWidth: 0 })).toBeNull();
    expect(computeTerminalGeometry({ ...base, cellHeight: 0 })).toBeNull();
  });

  it("honors proposed dimensions at the 80-column floor without scaling", () => {
    const above = computeTerminalGeometry({
      proposedCols: 100,
      proposedRows: 30,
      hostWidth: 390,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 13,
    });
    expect(above).toEqual({
      cols: 100,
      rows: 30,
      scale: 1,
      logicalWidth: 390,
      logicalHeight: 400,
    });

    const at = computeTerminalGeometry({
      proposedCols: 80,
      proposedRows: 24,
      hostWidth: 390,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 13,
    });
    expect(at).toEqual({
      cols: 80,
      rows: 24,
      scale: 1,
      logicalWidth: 390,
      logicalHeight: 400,
    });
  });

  it("grows the column target as the font shrinks below the floor, capped at the floor for oversized fonts", () => {
    const atMax = computeTerminalGeometry({
      proposedCols: 40,
      proposedRows: 12,
      hostWidth: 1000,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 20,
    });
    expect(atMax?.cols).toBe(80);

    const atDefault = computeTerminalGeometry({
      proposedCols: 40,
      proposedRows: 12,
      hostWidth: 1000,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 13,
    });
    expect(atDefault?.cols).toBe(87);

    const aboveMax = computeTerminalGeometry({
      proposedCols: 40,
      proposedRows: 12,
      hostWidth: 1000,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 22,
    });
    expect(aboveMax?.cols).toBe(80);
  });

  it("clamps the scale to 1 when the host is wide enough to fit logicalCols * cellWidth", () => {
    const result = computeTerminalGeometry({
      proposedCols: 40,
      proposedRows: 12,
      hostWidth: 1000,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 13,
    });
    expect(result).not.toBeNull();
    if (!result) return;
    expect(result.scale).toBe(1);
    expect(result.logicalWidth).toBe(1000);
    expect(result.logicalHeight).toBe(400);
  });

  it("scales the geometry down to hostWidth when the host is narrower than logicalCols * cellWidth", () => {
    const result = computeTerminalGeometry({
      proposedCols: 40,
      proposedRows: 12,
      hostWidth: 390,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 13,
    });
    expect(result).not.toBeNull();
    if (!result) return;
    expect(result.cols).toBe(87);
    const expectedScale = 390 / (87 * 8);
    expect(result.scale).toBeCloseTo(expectedScale);
    expect(result.scale).toBeLessThan(1);
    expect(result.logicalWidth).toBeCloseTo(390 / expectedScale);
    expect(result.logicalHeight).toBeCloseTo(400 / expectedScale);
  });

  it("derives rows from hostHeight and the clamped scale, with a minimum of 1", () => {
    const sized = computeTerminalGeometry({
      proposedCols: 40,
      proposedRows: 12,
      hostWidth: 390,
      hostHeight: 400,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 13,
    });
    expect(sized).not.toBeNull();
    if (!sized) return;
    expect(sized.rows).toBe(Math.max(1, Math.ceil(400 / (17 * sized.scale))));

    const tiny = computeTerminalGeometry({
      proposedCols: 40,
      proposedRows: 12,
      hostWidth: 390,
      hostHeight: 1,
      cellWidth: 8,
      cellHeight: 17,
      fontSize: 13,
    });
    expect(tiny).not.toBeNull();
    if (!tiny) return;
    expect(tiny.rows).toBe(1);
  });

  it("always returns a positive integer for rows across sampled inputs", () => {
    const samples = [
      {
        proposedCols: 40,
        proposedRows: 12,
        hostWidth: 390,
        hostHeight: 400,
        cellWidth: 8,
        cellHeight: 17,
        fontSize: 13,
      },
      {
        proposedCols: 40,
        proposedRows: 12,
        hostWidth: 1000,
        hostHeight: 400,
        cellWidth: 8,
        cellHeight: 17,
        fontSize: 13,
      },
      {
        proposedCols: 1,
        proposedRows: 1,
        hostWidth: 390,
        hostHeight: 1,
        cellWidth: 8,
        cellHeight: 17,
        fontSize: 13,
      },
      {
        proposedCols: 60,
        proposedRows: 20,
        hostWidth: 500,
        hostHeight: 200,
        cellWidth: 10,
        cellHeight: 20,
        fontSize: 10,
      },
    ];
    for (const sample of samples) {
      const result = computeTerminalGeometry(sample);
      expect(result).not.toBeNull();
      if (!result) continue;
      expect(Number.isInteger(result.rows)).toBe(true);
      expect(result.rows).toBeGreaterThan(0);
    }
  });
});
