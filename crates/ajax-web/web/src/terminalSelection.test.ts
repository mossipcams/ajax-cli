import { describe, it, expect } from "vitest";
import { cellAtPoint, orderedSelection } from "./terminalGestures";

describe("cellAtPoint", () => {
  const gridW = 800;
  const gridH = 480;
  const cols = 80;
  const rows = 24;

  it("maps a point inside the grid to the correct cell", () => {
    expect(cellAtPoint(105, 95, gridW, gridH, cols, rows)).toEqual({ col: 10, row: 4 });
  });

  it("clamps coordinates beyond the right and bottom edges", () => {
    expect(cellAtPoint(900, 500, gridW, gridH, cols, rows)).toEqual({ col: 79, row: 23 });
  });

  it("clamps coordinates below zero to the origin cell", () => {
    expect(cellAtPoint(-10, -5, gridW, gridH, cols, rows)).toEqual({ col: 0, row: 0 });
  });

  it("returns undefined for zero or negative grid dimensions", () => {
    expect(cellAtPoint(10, 10, 0, gridH, cols, rows)).toBeUndefined();
    expect(cellAtPoint(10, 10, gridW, -1, cols, rows)).toBeUndefined();
  });

  it("returns undefined for non-finite grid dimensions", () => {
    expect(cellAtPoint(10, 10, NaN, gridH, cols, rows)).toBeUndefined();
    expect(cellAtPoint(10, 10, gridW, Infinity, cols, rows)).toBeUndefined();
  });

  it("returns undefined for non-positive cols or rows", () => {
    expect(cellAtPoint(10, 10, gridW, gridH, 0, rows)).toBeUndefined();
    expect(cellAtPoint(10, 10, gridW, gridH, cols, 0)).toBeUndefined();
    expect(cellAtPoint(10, 10, gridW, gridH, 1.5, rows)).toBeUndefined();
  });
});

describe("orderedSelection", () => {
  it("keeps forward ranges in reading order", () => {
    const a = { col: 2, row: 1 };
    const b = { col: 10, row: 3 };
    expect(orderedSelection(a, b)).toEqual({ start: a, end: b });
  });

  it("swaps backward ranges on the same row", () => {
    const a = { col: 10, row: 2 };
    const b = { col: 3, row: 2 };
    expect(orderedSelection(a, b)).toEqual({ start: b, end: a });
  });

  it("swaps when the higher row was touched first", () => {
    const a = { col: 5, row: 8 };
    const b = { col: 1, row: 2 };
    expect(orderedSelection(a, b)).toEqual({ start: b, end: a });
  });
});
