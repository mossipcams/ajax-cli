import { describe, it, expect } from "vitest";
import {
  captureTranscriptGeometry,
  isSessionKeyboardOpen,
  layoutViewportShrinksWithKeyboard,
  restoreTranscriptGeometry,
  sessionKeyboardPadding,
  sessionVisibleHeight,
  transcriptAtBottom,
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

  it("detects transcript live edge within pin threshold", () => {
    expect(transcriptAtBottom(753, 1200, 400)).toBe(true);
    expect(transcriptAtBottom(400, 1200, 400)).toBe(false);
  });

  it("restores live bottom after keyboard dismiss", () => {
    const node = document.createElement("div");
    Object.defineProperty(node, "scrollHeight", { configurable: true, value: 1200 });
    Object.defineProperty(node, "clientHeight", { configurable: true, value: 400 });
    Object.defineProperty(node, "scrollTop", { configurable: true, writable: true, value: 800 });

    restoreTranscriptGeometry(node, captureTranscriptGeometry(node));
    expect(node.scrollTop).toBe(1200);
  });

  it("preserves history scrollTop and applies scrollHeight delta above viewport", () => {
    const node = document.createElement("div");
    Object.defineProperty(node, "scrollHeight", { configurable: true, value: 1400 });
    Object.defineProperty(node, "clientHeight", { configurable: true, value: 400 });
    Object.defineProperty(node, "scrollTop", { configurable: true, writable: true, value: 320 });

    const before = {
      scrollTop: 120,
      scrollHeight: 1200,
      clientHeight: 400,
      atBottom: false,
    };
    restoreTranscriptGeometry(node, before);
    expect(node.scrollTop).toBe(320);
  });
});
