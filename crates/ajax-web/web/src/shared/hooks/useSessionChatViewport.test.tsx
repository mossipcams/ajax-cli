import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSessionChatViewport } from "./useSessionChatViewport";

const keyboardState = vi.hoisted(() => ({
  keyboardOpen: false,
  keyboardHeight: 0,
  innerHeight: 800,
  visualViewportHeight: 800,
}));

vi.mock("./useMobileKeyboard", () => ({
  useMobileKeyboard: () => ({
    isMobile: true,
    keyboardOpen: keyboardState.keyboardOpen,
    keyboardHeight: keyboardState.keyboardHeight,
    innerHeight: keyboardState.innerHeight,
    visualViewportHeight: keyboardState.visualViewportHeight,
  }),
}));

describe("useSessionChatViewport", () => {
  const rafQueue: FrameRequestCallback[] = [];

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
  });

  afterEach(() => {
    document.documentElement.removeAttribute("data-session-viewport");
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
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1200 });
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });
    Object.defineProperty(thread, "scrollTop", { configurable: true, writable: true, value: 800 });

    const composer = document.createElement("textarea");
    Object.defineProperty(composer, "offsetHeight", { configurable: true, value: 44 });

    const threadRef = { current: thread };
    const composerRef = { current: composer };
    const pinnedRef = { current: pinned };

    const view = renderHook(() =>
      useSessionChatViewport({ threadRef, composerRef, pinnedRef }),
    );
    return { view, thread, composer, pinnedRef };
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

    Object.defineProperty(thread, "scrollTop", { configurable: true, writable: true, value: 400 });
    thread.dispatchEvent(new Event("scroll"));

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    keyboardState.visualViewportHeight = 800;
    view.rerender();
    flushLayoutSettle();

    expect(thread.scrollTop).toBe(thread.scrollHeight);
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

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    keyboardState.visualViewportHeight = 800;
    view.rerender();

    act(() => {
      Object.defineProperty(thread, "clientHeight", { configurable: true, value: 320 });
      Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1200 });
      const cb = rafQueue.shift();
      cb?.(0);
      expect(thread.scrollTop).toBe(1200);
    });

    act(() => {
      Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });
      const cb = rafQueue.shift();
      cb?.(0);
      expect(thread.scrollTop).toBe(1200);
    });

    flushLayoutSettle(4);
    expect(thread.scrollTop).toBe(thread.scrollHeight);
  });

  it("does not adjust scrollTop during settle when reading history", () => {
    const { view, thread } = mountHook(false);
    thread.scrollTop = 120;
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
    thread.scrollTop = 120;
    thread.dispatchEvent(new Event("scroll"));

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    view.rerender();

    thread.scrollTop = 60;
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
    thread.scrollTop = 200;
    thread.dispatchEvent(new Event("scroll"));

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    view.rerender();

    thread.scrollTop = 999;
    thread.dispatchEvent(new Event("scroll"));

    flushLayoutSettle();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    view.rerender();
    flushLayoutSettle();

    expect(thread.scrollTop).toBe(200);
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
    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1200 });
    Object.defineProperty(thread, "clientHeight", { configurable: true, value: 400 });
    Object.defineProperty(thread, "scrollTop", { configurable: true, writable: true, value: 800 });

    const composer = document.createElement("textarea");
    Object.defineProperty(composer, "offsetHeight", { configurable: true, value: 44 });

    const threadRef = { current: thread };
    const composerRef = { current: composer };
    const pinnedRef = { current: true };

    const view = renderHook(() =>
      useSessionChatViewport({ threadRef, composerRef, pinnedRef, onRestoreLiveEdge }),
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
