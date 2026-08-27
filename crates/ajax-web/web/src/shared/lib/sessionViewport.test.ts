import { describe, it, expect } from "vitest";
import {
  isSessionKeyboardOpen,
  layoutViewportShrinksWithKeyboard,
  pinTranscriptToLiveEdge,
  restoreTranscriptGeometry,
  sessionKeyboardPadding,
  sessionVisibleHeight,
  transcriptAtBottom,
  transcriptAtLiveEdge,
  SESSION_PIN_THRESHOLD_PX,
} from "./sessionViewport";

describe("sessionViewport", () => {
  function thread(scrollTop: number, scrollHeight: number, clientHeight: number): HTMLDivElement {
    const node = document.createElement("div");
    node.className = "session-thread";
    Object.defineProperty(node, "scrollHeight", { configurable: true, value: scrollHeight });
    Object.defineProperty(node, "clientHeight", { configurable: true, value: clientHeight });
    node.scrollTop = scrollTop;
    return node;
  }

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

  it("detects transcript live edge from scrollTop with AoE slop", () => {
    expect(transcriptAtLiveEdge(thread(800, 1200, 400))).toBe(true);
    expect(transcriptAtLiveEdge(thread(800 - SESSION_PIN_THRESHOLD_PX, 1200, 400))).toBe(true);
    expect(transcriptAtLiveEdge(thread(400, 1200, 400))).toBe(false);
  });

  it("transcriptAtBottom matches scroll metrics with AoE slop", () => {
    expect(transcriptAtBottom(784, 1200, 400)).toBe(true);
    expect(transcriptAtBottom(783, 1200, 400)).toBe(false);
    expect(transcriptAtBottom(400, 1200, 400)).toBe(false);
  });

  it("restores live bottom via stick-to-bottom after keyboard dismiss", () => {
    const node = thread(120, 2000, 400);
    restoreTranscriptGeometry(node, { atBottom: true });
    expect(node.scrollTop).toBe(1600);
  });

  it("preserves history scrollTop and applies scrollHeight delta above viewport", () => {
    const node = thread(320, 1400, 400);
    restoreTranscriptGeometry(node, {
      atBottom: false,
      scrollTop: 120,
      scrollHeight: 1200,
    });
    expect(node.scrollTop).toBe(320);
  });

  it("pins live edge by assigning scrollTop to scrollHeight minus clientHeight", () => {
    const node = thread(0, 1200, 400);
    pinTranscriptToLiveEdge(node);
    expect(node.scrollTop).toBe(800);
  });
});
