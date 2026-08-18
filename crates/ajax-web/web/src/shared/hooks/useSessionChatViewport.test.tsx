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
    vi.useFakeTimers();
  });

  afterEach(() => {
    document.documentElement.removeAttribute("data-session-viewport");
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  function flushRaf(max = 5) {
    act(() => {
      for (let i = 0; i < max && rafQueue.length; i++) {
        const cb = rafQueue.shift();
        cb?.(0);
      }
      vi.advanceTimersByTime(600);
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

  it("repins transcript to scrollHeight when keyboard closes while pinned", () => {
    const { view, thread } = mountHook(true);
    expect(view.result.current.surfaceStyle).toBeUndefined();

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    keyboardState.visualViewportHeight = 500;
    view.rerender();
    flushRaf();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    keyboardState.visualViewportHeight = 800;
    view.rerender();
    flushRaf();

    expect(thread.scrollTop).toBe(thread.scrollHeight);
  });

  it("repins when keyboard closes while pinned even if live edge drifted during open", () => {
    const { view, thread, pinnedRef } = mountHook(true);
    pinnedRef.current = true;
    Object.defineProperty(thread, "scrollTop", { configurable: true, writable: true, value: 400 });

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    view.rerender();
    flushRaf();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    view.rerender();
    flushRaf();

    expect(thread.scrollTop).toBe(thread.scrollHeight);
  });

  it("preserves scrollTop when reading history across keyboard close", () => {
    const { view, thread } = mountHook(false);
    thread.scrollTop = 120;

    keyboardState.keyboardOpen = true;
    keyboardState.keyboardHeight = 300;
    view.rerender();
    flushRaf();

    keyboardState.keyboardOpen = false;
    keyboardState.keyboardHeight = 0;
    view.rerender();
    flushRaf();

    expect(thread.scrollTop).toBe(120);
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
});
