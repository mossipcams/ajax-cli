import { describe, expect, it } from "vitest";
import {
  deleteBackward,
  insertAtSelection,
  moveCaret,
} from "./sessionDraftEdit";

describe("sessionDraftEdit", () => {
  it("inserts at caret and replaces selection", () => {
    expect(insertAtSelection({ value: "ab", selectionStart: 1, selectionEnd: 1 }, "X")).toEqual({
      value: "aXb",
      selectionStart: 2,
      selectionEnd: 2,
    });
    expect(insertAtSelection({ value: "abcd", selectionStart: 1, selectionEnd: 3 }, "X")).toEqual({
      value: "aXd",
      selectionStart: 2,
      selectionEnd: 2,
    });
  });

  it("deletes selection or previous character", () => {
    expect(deleteBackward({ value: "abc", selectionStart: 2, selectionEnd: 2 })).toEqual({
      value: "ac",
      selectionStart: 1,
      selectionEnd: 1,
    });
    expect(deleteBackward({ value: "abc", selectionStart: 1, selectionEnd: 3 })).toEqual({
      value: "a",
      selectionStart: 1,
      selectionEnd: 1,
    });
  });

  it("moves caret left/right/up/down across lines", () => {
    expect(moveCaret({ value: "ab", selectionStart: 1, selectionEnd: 1 }, "left")).toEqual({
      value: "ab",
      selectionStart: 0,
      selectionEnd: 0,
    });
    expect(moveCaret({ value: "a\nbc", selectionStart: 3, selectionEnd: 3 }, "up")).toEqual({
      value: "a\nbc",
      selectionStart: 1,
      selectionEnd: 1,
    });
    expect(moveCaret({ value: "ab\nc", selectionStart: 1, selectionEnd: 1 }, "down")).toEqual({
      value: "ab\nc",
      selectionStart: 4,
      selectionEnd: 4,
    });
  });
});
