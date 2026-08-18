import { describe, it, expect } from "vitest";
import {
  isSessionKeyboardOpen,
  layoutViewportShrinksWithKeyboard,
  sessionKeyboardPadding,
  sessionVisibleHeight,
} from "./sessionViewport";

describe("sessionViewport", () => {
  it("detects keyboard open when visualViewport shrinks well below full height", () => {
    expect(isSessionKeyboardOpen(800, 480)).toBe(true);
    expect(isSessionKeyboardOpen(800, 750)).toBe(false);
  });

  it("treats layout viewport shrink as already keyboard-aware (PWA / Android)", () => {
    expect(layoutViewportShrinksWithKeyboard(520, 500)).toBe(true);
    expect(sessionKeyboardPadding(520, 500, true)).toBe(0);
  });

  it("reserves bottom padding on iOS Safari (innerHeight constant, vv shrinks)", () => {
    expect(sessionKeyboardPadding(800, 500, true, 0)).toBe(300);
    expect(sessionKeyboardPadding(800, 500, true, 20)).toBe(280);
  });

  it("clears padding when keyboard is closed", () => {
    expect(sessionKeyboardPadding(800, 500, false)).toBe(0);
  });

  it("uses visualViewport height as the visible band when usable", () => {
    expect(sessionVisibleHeight(800, 480)).toBe(480);
    expect(sessionVisibleHeight(800, 0)).toBe(800);
  });
});
