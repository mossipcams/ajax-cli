import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useChatViewport } from "./useChatViewport";
import {
  expectThreadAtLiveEdge,
  installTranscriptScrollIntoViewMock,
  mockScrollByToAdjustScrollTop,
  stubScrollMetrics,
  syncTranscriptLayoutRects,
} from "./transcriptLayout.testHelpers";

const keyboardState = vi.hoisted(() => ({
  keyboardOpen: false,
  keyboardHeight: 0,
  innerHeight: 800,
  visualViewportHeight: 800,
}));

vi.mock("@/shared/hooks/useMobileKeyboard", () => ({
  useMobileKeyboard: () => ({
    isMobile: true,
    keyboardOpen: keyboardState.keyboardOpen,
    keyboardHeight: keyboardState.keyboardHeight,
    innerHeight: keyboardState.innerHeight,
    visualViewportHeight: keyboardState.visualViewportHeight,
  }),
}));

describe("useChatViewport", () => {
  const rafQueue: FrameRequestCallback[] = [];
  let restoreScrollIntoView: (() => void) | undefined;
  let restoreScrollBy: (() => void) | undefined;

  beforeEach(() => {
    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    keyboardState.innerHeight = 800;
    keyboardState.visualViewportHeight = 800;
    document.documentElement.removeAttribute("data-session-viewport");
    rafQueue.length = 0;
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      rafQueue.push(cb);
      return rafQueue.length;
    });
    vi.stubGlobal("cancelAnimationFrame", vi.fn());
    restoreScrollIntoView = installTranscriptScrollIntoViewMock();
    restoreScrollBy = mockScrollByToAdjustScrollTop();
  });

  afterEach(() => {
    document.documentElement.removeAttribute("data-session-viewport");
    restoreScrollIntoView?.();
    restoreScrollBy?.();
    vi.unstubAllGlobals();
  });

  function flushLayoutSettle(max = 8) {
    act(() => {
      for (let i = 0; i < max && rafQueue.length; i++) {
        const cb = rafQueue.shift();
        cb?.(0);
      }
    });
  }

  function mountHook(pinned = true) {
    const thread = document.createElement("div");
    thread.className = "session-thread";
    const inner = document.createElement("div");
    inner.className = "session-thread-inner";
    const row = document.createElement("article");
    inner.appendChild(row);
    thread.appendChild(inner);
    stubScrollMetrics(thread, 1200, 400, pinned ? 800 : 120);

    const composer = document.createElement("textarea");
    Object.defineProperty(composer, "offsetHeight", { configurable: true, value: 44 });

    const threadRef = { current: thread };
    const composerRef = { current: composer };
    const pinnedRef = { current: pinned };
    const ignoreScrollIntentRef = { current: false };
    const layoutTransitionRef = { current: false };

    const view = renderHook(() =>
      useChatViewport({
        threadRef,
        composerRef,
        pinnedRef,
        ignoreScrollIntentRef,
        layoutTransitionRef,
      }),
    );
    return { view, thread, inner, row, composer, pinnedRef, ignoreScrollIntentRef, layoutTransitionRef };
  }

  it("claims session viewport ownership on mount", () => {
    mountHook();
    expect(document.documentElement.getAttribute("data-session-viewport")).toBe("owned");
  });

  it("restores live bottom once after keyboard closes while pinned", () => {
    const { view, thread } = mountHook(true);
    expect(view.result.current.surfaceStyle).toBeUndefined();

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    keyboardState.visualViewportHeight = 500;
    view.rerender();
    flushLayoutSettle();

    stubScrollMetrics(thread, 1200, 400, 400);
    thread.dispatchEvent(new Event("scroll"));

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    keyboardState.visualViewportHeight = 800;
    view.rerender();
    flushLayoutSettle();

    expectThreadAtLiveEdge(thread);
  });

  it("pins live edge each settle frame while keyboard band closes", () => {
    const { view, thread } = mountHook(true);
    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    keyboardState.visualViewportHeight = 500;
    view.rerender();

    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 250 });
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1200 });
    thread.scrollTop = 800;
    syncTranscriptLayoutRects(thread);

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    keyboardState.visualViewportHeight = 800;
    view.rerender();

    act(() => {
      Object.defineProperty(thread, "clientHeight", { configurable: true, value: 320 });
      Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1200 });
      const cb = rafQueue.shift();
      cb?.(0);
      expectThreadAtLiveEdge(thread);
    });

    act(() => {
      Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });
      syncTranscriptLayoutRects(thread);
      const cb = rafQueue.shift();
      cb?.(0);
      expectThreadAtLiveEdge(thread);
    });

    flushLayoutSettle(4);
    expectThreadAtLiveEdge(thread);
  });

  it("does not adjust scrollTop during settle when reading history", () => {
    const { view, thread } = mountHook(false);
    stubScrollMetrics(thread, 1200, 400, 120);
    thread.dispatchEvent(new Event("scroll"));

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    view.rerender();
    flushLayoutSettle();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    view.rerender();

    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 500 });
    thread.scrollTop = 60;
    syncTranscriptLayoutRects(thread);

    act(() => {
      const cb = rafQueue.shift();
      cb?.(0);
      expect(thread.scrollTop).toBe(60);
    });

    flushLayoutSettle(4);
    expect(thread.scrollTop).toBe(120);
  });

  it("preserves scrollTop when reading history across keyboard close", () => {
    const { view, thread } = mountHook(false);
    stubScrollMetrics(thread, 1200, 400, 120);
    thread.dispatchEvent(new Event("scroll"));

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    view.rerender();

    thread.scrollTop = 60;
    syncTranscriptLayoutRects(thread);
    thread.dispatchEvent(new Event("scroll"));

    flushLayoutSettle();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    view.rerender();
    flushLayoutSettle();

    expect(thread.scrollTop).toBe(120);
  });

  it("ignores Safari resize-generated scroll during keyboard transition", () => {
    const { view, thread } = mountHook(false);
    stubScrollMetrics(thread, 1200, 400, 200);
    thread.dispatchEvent(new Event("scroll"));

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    view.rerender();

    thread.scrollTop = 999;
    syncTranscriptLayoutRects(thread);
    thread.dispatchEvent(new Event("scroll"));

    flushLayoutSettle();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    view.rerender();
    flushLayoutSettle();

    expect(thread.scrollTop).toBe(200);
  });

  it("restores pre-keyboard scroll when pinnedRef drifts true during keyboard (#930)", () => {
    const { view, thread, pinnedRef } = mountHook(false);
    stubScrollMetrics(thread, 13108, 400, 6685);
    thread.dispatchEvent(new Event("scroll"));

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    view.rerender();
    pinnedRef.current = true;
    flushLayoutSettle();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    view.rerender();
    flushLayoutSettle();

    expect(thread.scrollTop).toBe(6685);
  });

  it("applies surface paddingBottom on iOS Safari keyboard band", () => {
    const { view } = mountHook();
    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    keyboardState.innerHeight = 800;
    keyboardState.visualViewportHeight = 500;
    view.rerender();
    expect(view.result.current.surfaceStyle).toEqual({ paddingBottom: 300 });
  });

  it("calls onRestoreLiveEdge after keyboard close restores live bottom", () => {
    const onRestoreLiveEdge = vi.fn();
    const thread = document.createElement("div");
    thread.className = "session-thread";
    const inner = document.createElement("div");
    inner.className = "session-thread-inner";
    inner.appendChild(document.createElement("article"));
    thread.appendChild(inner);
    stubScrollMetrics(thread, 1200, 400, 800);

    const composer = document.createElement("textarea");
    Object.defineProperty(composer, "offsetHeight", { configurable: true, value: 44 });

    const threadRef = { current: thread };
    const composerRef = { current: composer };
    const pinnedRef = { current: true };
    const ignoreScrollIntentRef = { current: false };
    const layoutTransitionRef = { current: false };

    const view = renderHook(() =>
      useChatViewport({
        threadRef,
        composerRef,
        pinnedRef,
        ignoreScrollIntentRef,
        layoutTransitionRef,
        onRestoreLiveEdge,
      }),
    );

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    keyboardState.visualViewportHeight = 500;
    view.rerender();
    flushLayoutSettle();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    keyboardState.visualViewportHeight = 800;
    view.rerender();
    flushLayoutSettle();

    expect(onRestoreLiveEdge).toHaveBeenCalledOnce();
  });
});
