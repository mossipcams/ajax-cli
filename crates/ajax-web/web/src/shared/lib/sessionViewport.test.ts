import { describe, it, expect } from "vitest";
import {
  isSessionKeyboardOpen,
  layoutViewportShrinksWithKeyboard,
  pinTranscriptToLiveEdge,
  restoreTranscriptGeometry,
  sessionKeyboardPadding,
  sessionSurfaceStyle,
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

  it("sessionSurfaceStyle reserves keyboard inset on iOS Safari via paddingBottom", () => {
    expect(sessionSurfaceStyle(800, 800, false)).toBeUndefined();
    expect(sessionSurfaceStyle(800, 500, true)).toEqual({ paddingBottom: 300 });
    expect(sessionSurfaceStyle(500, 480, true)).toBeUndefined();
  });

  it("uses visualViewport height as the visible band when usable", () => {
    expect(sessionVisibleHeight(800, 480)).toBe(480);
    expect(sessionVisibleHeight(800, 0)).toBe(800);
  });

  it("treats scrollTop = 0 as the live edge with negative offsets away (AoE slop)", () => {
    expect(transcriptAtLiveEdge(thread(0, 1200, 400))).toBe(true);
    expect(transcriptAtLiveEdge(thread(-SESSION_PIN_THRESHOLD_PX, 1200, 400))).toBe(true);
    expect(transcriptAtLiveEdge(thread(-400, 1200, 400))).toBe(false);
  });

  it("transcriptAtBottom matches reversed scroll metrics with AoE slop", () => {
    expect(transcriptAtBottom(0, 1200, 400)).toBe(true);
    expect(transcriptAtBottom(-SESSION_PIN_THRESHOLD_PX, 1200, 400)).toBe(true);
    expect(transcriptAtBottom(-SESSION_PIN_THRESHOLD_PX - 1, 1200, 400)).toBe(false);
    expect(transcriptAtBottom(-400, 1200, 400)).toBe(false);
  });

  it("restores live bottom via stick-to-bottom after keyboard dismiss", () => {
    const node = thread(-120, 2000, 400);
    restoreTranscriptGeometry(node, { atBottom: true });
    expect(node.scrollTop).toBe(0);
  });

  it("preserves distance from the live edge and applies scrollHeight delta above viewport", () => {
    const node = thread(-320, 1400, 400);
    restoreTranscriptGeometry(node, {
      atBottom: false,
      scrollTop: -120,
      scrollHeight: 1200,
    });
    expect(node.scrollTop).toBe(-320);
  });

  it("pins live edge by assigning scrollTop to zero (reversed coordinates)", () => {
    const node = thread(-800, 1200, 400);
    pinTranscriptToLiveEdge(node);
    expect(node.scrollTop).toBe(0);
  });

  function webkitRoundingThread(settlesAt: (writeIndex: number) => number): {
    node: HTMLDivElement;
    writes: () => number;
  } {
    const node = document.createElement("div");
    let stored = settlesAt(0);
    let writeIndex = -1;
    Object.defineProperty(node, "scrollHeight", { configurable: true, value: 1200 });
    Object.defineProperty(node, "clientHeight", { configurable: true, value: 400 });
    Object.defineProperty(node, "scrollTop", {
      configurable: true,
      get() {
        return stored;
      },
      set(value: number) {
        writeIndex += 1;
        // Simulate WebKit column-reverse rounding: the stored value is what
        // the browser settles at after each programmatic write.
        stored = value === 0 ? settlesAt(writeIndex) : value;
      },
    });
    return { node, writes: () => writeIndex + 1 };
  }

  it("re-pins a subpixel -1 live-edge drift on WebKit until it reads back 0", () => {
    // First scrollTop = 0 write rounds to -1; the second settles exactly at 0.
    const { node, writes } = webkitRoundingThread((i) => (i === 0 ? -1 : 0));
    pinTranscriptToLiveEdge(node);
    expect(node.scrollTop).toBe(0);
    expect(writes()).toBeGreaterThanOrEqual(2);
  });

  it("does not force-pinning when drift exceeds the correction band (reader intent)", () => {
    // A write settles at -20: beyond LIVE_EDGE_PIN_BAND_PX, so no re-pin loop.
    const { node, writes } = webkitRoundingThread(() => -20);
    pinTranscriptToLiveEdge(node);
    expect(node.scrollTop).toBe(-20);
    expect(writes()).toBe(1);
  });
});
