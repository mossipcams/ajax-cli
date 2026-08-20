import { describe, it, expect, afterEach } from "vitest";
import {
  isWordChar,
  wordBoundsAtCol,
  selectionRangeBetweenCells,
  selectionRangeFromWordAnchor,
} from "./terminalTouchSelection";
import {
  isTerminalDoubleTapPending,
  isTerminalSelecting,
  isTerminalTouchTarget,
  setTerminalDoubleTapPending,
  setTerminalSelecting,
  shouldSuppressPageSwipe,
} from "@/shared/lib/terminalSelecting";

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

describe("terminalSelecting flag", () => {
  afterEach(() => {
    setTerminalSelecting(false);
    setTerminalDoubleTapPending(false);
  });

  it("toggles the document dataset used to suppress page swipe", () => {
    expect(isTerminalSelecting()).toBe(false);
    setTerminalSelecting(true);
    expect(isTerminalSelecting()).toBe(true);
    expect(document.documentElement.dataset.ajaxTerminalSelecting).toBe("1");
    setTerminalSelecting(false);
    expect(isTerminalSelecting()).toBe(false);
    expect(document.documentElement.dataset.ajaxTerminalSelecting).toBeUndefined();
  });

  it("suppresses swipe for pending double-tap only on terminal targets", () => {
    const host = document.createElement("div");
    host.className = "terminal-host";
    const outside = document.createElement("div");
    expect(isTerminalTouchTarget(host)).toBe(true);
    expect(isTerminalTouchTarget(outside)).toBe(false);

    setTerminalDoubleTapPending(true);
    expect(isTerminalDoubleTapPending()).toBe(true);
    expect(shouldSuppressPageSwipe(host)).toBe(true);
    expect(shouldSuppressPageSwipe(outside)).toBe(false);

    setTerminalDoubleTapPending(false);
    expect(shouldSuppressPageSwipe(host)).toBe(false);

    setTerminalSelecting(true);
    expect(shouldSuppressPageSwipe(outside)).toBe(true);
  });
});
