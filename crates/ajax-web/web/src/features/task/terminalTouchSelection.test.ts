import { describe, it, expect } from "vitest";
import {
  isWordChar,
  wordBoundsAtCol,
  selectionRangeBetweenCells,
  selectionRangeFromWordAnchor,
} from "./terminalTouchSelection";

describe("isWordChar", () => {
  it("accepts alphanumerics, hyphen, underscore, and non-ascii", () => {
    expect(isWordChar("a")).toBe(true);
    expect(isWordChar("Z")).toBe(true);
    expect(isWordChar("9")).toBe(true);
    expect(isWordChar("-")).toBe(true);
    expect(isWordChar("_")).toBe(true);
    expect(isWordChar("é")).toBe(true);
  });

  it("rejects spaces and punctuation", () => {
    expect(isWordChar(" ")).toBe(false);
    expect(isWordChar(".")).toBe(false);
    expect(isWordChar("")).toBe(false);
  });
});

describe("wordBoundsAtCol", () => {
  it("returns word span around the contact column", () => {
    const bounds = wordBoundsAtCol("hello world", 6);
    expect(bounds).toEqual({ start: 6, end: 11 });
  });

  it("returns null when column is past trimmed content", () => {
    expect(wordBoundsAtCol("hi   ", 4)).toBeNull();
    expect(wordBoundsAtCol("", 0)).toBeNull();
  });
});

describe("selectionRangeBetweenCells", () => {
  const cols = 80;

  it("selects forward on the same row", () => {
    expect(selectionRangeBetweenCells(5, 2, 10, 2, cols)).toEqual({
      col: 5,
      row: 2,
      length: 6,
    });
  });

  it("selects backward on the same row", () => {
    expect(selectionRangeBetweenCells(10, 2, 5, 2, cols)).toEqual({
      col: 5,
      row: 2,
      length: 6,
    });
  });

  it("spans rows using linear buffer offsets", () => {
    expect(selectionRangeBetweenCells(78, 0, 1, 1, cols)).toEqual({
      col: 78,
      row: 0,
      length: 4,
    });
  });
});

describe("selectionRangeFromWordAnchor", () => {
  const cols = 80;

  it("keeps the full word while the finger stays inside it", () => {
    expect(selectionRangeFromWordAnchor(7, 20, 13, 2, 2, cols)).toEqual({
      col: 7,
      row: 2,
      length: 13,
    });
  });

  it("extends past the word end toward a later cell", () => {
    expect(selectionRangeFromWordAnchor(7, 20, 30, 2, 2, cols)).toEqual({
      col: 7,
      row: 2,
      length: 24,
    });
  });
});
