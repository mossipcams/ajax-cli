import { describe, it, expect, vi, afterEach, type UIEvent } from "vitest";
import { renderHook, act } from "@testing-library/react";
import * as sessionViewport from "@/shared/lib/sessionViewport";
import {
  useChatScroll,
  PIN_THRESHOLD_PX,
  resetReadingPositionsForTests,
} from "./useChatScroll";
import { stubScrollMetrics } from "./transcriptLayout.testHelpers";
import { transcriptAtLiveEdge, transcriptScrollBottom } from "@/shared/lib/sessionViewport";
import { AUTO_LOAD_COOLDOWN_MS } from "./historyScroll";

describe("useChatScroll", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    resetReadingPositionsForTests();
  });

  function mountScroll(
    sessionKey = "s1",
    revision = 0,
    initialHistoryScroll?: {
      hasEarlier: boolean;
      revealEarlier: () => number;
      windowGeneration: number;
    },
    initialMetrics?: [number, number, number],
  ) {
    const thread = document.createElement("div");
    thread.className = "session-thread";
    const inner = document.createElement("div");
    inner.className = "session-thread-inner";
    inner.appendChild(document.createElement("article"));
    thread.appendChild(inner);
    stubScrollMetrics(thread, ...(initialMetrics ?? [1200, 400, 0]));

    const threadRef = { current: thread };
    const layoutTransitionRef = { current: false };

    const view = renderHook(
      ({ rev, key, historyScroll }) =>
        useChatScroll({
          threadRef,
          revision: rev,
          sessionKey: key,
          layoutTransitionRef,
          historyScroll,
        }),
      {
        initialProps: {
          rev: revision,
          key: sessionKey,
          historyScroll: initialHistoryScroll,
        },
      },
    );

    return { view, thread, threadRef, layoutTransitionRef };
  }

  it("sticks to live edge on first paint", () => {
    const { thread } = mountScroll();
    expect(thread.scrollTop).toBe(0);
    expect(transcriptAtLiveEdge(thread, PIN_THRESHOLD_PX)).toBe(true);
  });

  it("samples pin from scroll with 16px slop from the live edge", () => {
    const { thread, view } = mountScroll();
    stubScrollMetrics(thread, 1200, 400, 0);
    const { onThreadScroll } = view.result.current;

    act(() => {
      thread.scrollTop = -20;
      onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    expect(view.result.current.behind).toBe(true);

    act(() => {
      thread.scrollTop = -10;
      onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    expect(view.result.current.behind).toBe(false);
  });

  it("re-pins on session identity change", () => {
    const { thread, view } = mountScroll("s1");
    thread.scrollTop = -80;
    const { onThreadScroll } = view.result.current;
    act(() => {
      onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    act(() => {
      view.rerender({ rev: 0, key: "s2", historyScroll: undefined });
    });

    expect(thread.scrollTop).toBe(0);
    expect(view.result.current.behind).toBe(false);
  });

  it("does not move scrollTop when unpinned and revision grows", () => {
    const { thread, view } = mountScroll("s1", 0);
    stubScrollMetrics(thread, 1200, 400, -120);
    act(() => {
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1400 });
    act(() => {
      view.rerender({ rev: 1, key: "s1", historyScroll: undefined });
    });

    expect(thread.scrollTop).toBe(-120);
  });

  it("keeps the reading position when the head above the thread resizes (#1636)", () => {
    const resizeCallbacks: Array<() => void> = [];
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: () => void) {
          resizeCallbacks.push(callback);
        }
        observe() {}
        disconnect() {}
      },
    );
    let top = 0;
    vi.spyOn(HTMLDivElement.prototype, "getBoundingClientRect").mockImplementation(
      () => ({ top }) as DOMRect,
    );
    const { thread, view } = mountScroll("s1", 0);
    stubScrollMetrics(thread, 1200, 400, -120);
    act(() => {
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    top = 26;
    act(() => resizeCallbacks[resizeCallbacks.length - 1]());
    expect(thread.scrollTop).toBe(-94);

    top = 0;
    act(() => resizeCallbacks[resizeCallbacks.length - 1]());
    expect(thread.scrollTop).toBe(-120);
  });

  it("restores scroll position after prepend-style history reveal", () => {
    let windowGeneration = 0;
    const { view, thread } = mountScroll("s1", 0, {
      hasEarlier: true,
      revealEarlier: () => {
        windowGeneration += 1;
        Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1500 });
        return 2;
      },
      windowGeneration: 0,
    });

    stubScrollMetrics(thread, 1200, 400, -180);
    act(() => {
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    act(() => {
      view.result.current.loadEarlier();
    });

    act(() => {
      view.rerender({
        rev: 0,
        key: "s1",
        historyScroll: {
          hasEarlier: true,
          revealEarlier: () => 0,
          windowGeneration,
        },
      });
    });

    expect(thread.scrollTop).toBe(-180);
  });

  it("restores scroll position after prepend even when the window fits the viewport", () => {
    let windowGeneration = 0;
    const { view, thread } = mountScroll("s1", 0, {
      hasEarlier: true,
      revealEarlier: () => {
        windowGeneration += 1;
        Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 500 });
        return 2;
      },
      windowGeneration: 0,
    });

    stubScrollMetrics(thread, 300, 400, 0);
    expect(transcriptAtLiveEdge(thread, PIN_THRESHOLD_PX)).toBe(true);
    expect(view.result.current.behind).toBe(false);

    act(() => {
      view.result.current.loadEarlier();
    });

    act(() => {
      view.rerender({
        rev: 0,
        key: "s1",
        historyScroll: {
          hasEarlier: true,
          revealEarlier: () => 0,
          windowGeneration,
        },
      });
    });

    expect(thread.scrollTop).toBe(0);
  });

  it("does not pin to live edge while a prepend restore is pending", () => {
    const pinSpy = vi.spyOn(sessionViewport, "pinTranscriptToLiveEdge");
    const { view, thread } = mountScroll("s1", 0, {
      hasEarlier: true,
      revealEarlier: () => 2,
      windowGeneration: 0,
    });

    stubScrollMetrics(thread, 1200, 400, 0);
    pinSpy.mockClear();

    act(() => {
      view.result.current.loadEarlier();
    });

    act(() => {
      thread.querySelector("article")!.appendChild(document.createElement("span"));
    });

    expect(pinSpy).not.toHaveBeenCalled();
    pinSpy.mockRestore();
  });

  it("auto-loads earlier rows near the history edge when armed", () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const revealEarlier = vi.fn(() => 1);
    let windowGeneration = 0;
    const historyScroll = {
      hasEarlier: true,
      revealEarlier: () => {
        windowGeneration += 1;
        return revealEarlier();
      },
      get windowGeneration() {
        return windowGeneration;
      },
    };
    const { view, thread } = mountScroll("s1", 0, historyScroll);

    stubScrollMetrics(thread, 1200, 400, -400);
    act(() => {
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    stubScrollMetrics(thread, 1200, 400, -750);
    act(() => {
      vi.setSystemTime(AUTO_LOAD_COOLDOWN_MS + 10);
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    expect(revealEarlier).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });

  it("auto-loads on first approach to the history edge without a prior arming pass", () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const revealEarlier = vi.fn(() => 1);
    let windowGeneration = 0;
    const historyScroll = {
      hasEarlier: true,
      revealEarlier: () => {
        windowGeneration += 1;
        return revealEarlier();
      },
      get windowGeneration() {
        return windowGeneration;
      },
    };
    const { view, thread } = mountScroll("s1", 0, historyScroll);

    stubScrollMetrics(thread, 1200, 400, -750);
    act(() => {
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });
    expect(revealEarlier).not.toHaveBeenCalled();

    act(() => {
      vi.setSystemTime(AUTO_LOAD_COOLDOWN_MS + 10);
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });
    expect(revealEarlier).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });

  it("fills the viewport while the tail does not overflow and earlier history remains", () => {
    const revealEarlier = vi.fn(() => 2);
    let windowGeneration = 0;
    const historyScroll = {
      hasEarlier: true,
      revealEarlier: () => {
        windowGeneration += 1;
        return revealEarlier();
      },
      get windowGeneration() {
        return windowGeneration;
      },
    };
    const { view, thread } = mountScroll("s1", 0, historyScroll, [300, 400, 0]);

    expect(revealEarlier).toHaveBeenCalledOnce();

    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 500 });
    act(() => {
      view.rerender({
        rev: 0,
        key: "s1",
        historyScroll: { hasEarlier: true, revealEarlier: () => 2, windowGeneration },
      });
    });
    expect(revealEarlier).toHaveBeenCalledOnce();
    expect(thread.scrollTop).toBe(0);
  });

  it("restores the reading position after Chat → Diff → Back for the same session (#1629)", () => {
    const thread = document.createElement("div");
    thread.className = "session-thread";
    stubScrollMetrics(thread, 1200, 400, 0);
    const threadRef = { current: thread };
    const mountChat = (key: string) =>
      renderHook(() => useChatScroll({ threadRef, revision: 0, sessionKey: key }));

    const first = mountChat("s1");
    stubScrollMetrics(thread, 1200, 400, -120);
    act(() => {
      first.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });
    expect(first.result.current.behind).toBe(true);
    first.unmount();

    stubScrollMetrics(thread, 1400, 400, -120);
    const second = mountChat("s1");

    expect(
      Math.abs(transcriptScrollBottom(thread.scrollTop, thread.scrollHeight, thread.clientHeight) - 120),
    ).toBeLessThanOrEqual(4);
    expect(second.result.current.behind).toBe(true);
  });

  it("keeps the live edge on remount when the chat was pinned at unmount (#1629)", () => {
    const thread = document.createElement("div");
    thread.className = "session-thread";
    stubScrollMetrics(thread, 1200, 400, 0);
    const threadRef = { current: thread };
    const mountChat = (key: string) =>
      renderHook(() => useChatScroll({ threadRef, revision: 0, sessionKey: key }));

    const first = mountChat("s1");
    stubScrollMetrics(thread, 1200, 400, -120);
    act(() => {
      first.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });
    stubScrollMetrics(thread, 1200, 400, 0);
    act(() => {
      first.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });
    first.unmount();

    const second = mountChat("s1");

    expect(thread.scrollTop).toBe(0);
    expect(second.result.current.behind).toBe(false);
  });

  it("starts a different session at the live edge after a scrolled-up unmount (#1629)", () => {
    const thread = document.createElement("div");
    thread.className = "session-thread";
    stubScrollMetrics(thread, 1200, 400, 0);
    const threadRef = { current: thread };
    const mountChat = (key: string) =>
      renderHook(({ sessionKey }) => useChatScroll({ threadRef, revision: 0, sessionKey }), {
        initialProps: { sessionKey: key },
      });

    const first = mountChat("s1");
    stubScrollMetrics(thread, 1200, 400, -120);
    act(() => {
      first.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });
    first.unmount();

    stubScrollMetrics(thread, 1200, 400, -120);
    const second = mountChat("s2");

    expect(thread.scrollTop).toBe(0);
    expect(second.result.current.behind).toBe(false);
  });
});
