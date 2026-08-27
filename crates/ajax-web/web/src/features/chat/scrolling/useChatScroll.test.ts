import { describe, it, expect, vi, afterEach, type UIEvent } from "vitest";
import { renderHook, act } from "@testing-library/react";
import * as sessionViewport from "@/shared/lib/sessionViewport";
import { useChatScroll, PIN_THRESHOLD_PX } from "./useChatScroll";
import { liveEdgeScrollTop, stubScrollMetrics } from "./transcriptLayout.testHelpers";
import { transcriptAtLiveEdge } from "@/shared/lib/sessionViewport";
import { AUTO_LOAD_COOLDOWN_MS, HISTORY_PRELOAD_PX } from "./historyScroll";

describe("useChatScroll", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  function mountScroll(
    sessionKey = "s1",
    revision = 0,
    initialHistoryScroll?: {
      hasEarlier: boolean;
      revealEarlier: () => number;
      windowGeneration: number;
    },
  ) {
    const thread = document.createElement("div");
    thread.className = "session-thread";
    const inner = document.createElement("div");
    inner.className = "session-thread-inner";
    inner.appendChild(document.createElement("article"));
    thread.appendChild(inner);
    stubScrollMetrics(thread, 1200, 400, 0);

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
    expect(thread.scrollTop).toBe(liveEdgeScrollTop(1200, 400));
    expect(transcriptAtLiveEdge(thread, PIN_THRESHOLD_PX)).toBe(true);
  });

  it("samples pin from scroll with 16px slop", () => {
    const { thread, view } = mountScroll();
    stubScrollMetrics(thread, 1200, 400, liveEdgeScrollTop(1200, 400));
    const { onThreadScroll } = view.result.current;

    act(() => {
      thread.scrollTop = liveEdgeScrollTop(1200, 400) - 20;
      onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    expect(view.result.current.behind).toBe(true);

    act(() => {
      thread.scrollTop = liveEdgeScrollTop(1200, 400) - 10;
      onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    expect(view.result.current.behind).toBe(false);
  });

  it("re-pins on session identity change", () => {
    const { thread, view } = mountScroll("s1");
    thread.scrollTop = 80;
    const { onThreadScroll } = view.result.current;
    act(() => {
      onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    act(() => {
      view.rerender({ rev: 0, key: "s2", historyScroll: undefined });
    });

    expect(thread.scrollTop).toBe(liveEdgeScrollTop(1200, 400));
    expect(view.result.current.behind).toBe(false);
  });

  it("does not move scrollTop when unpinned and revision grows", () => {
    const { thread, view } = mountScroll("s1", 0);
    stubScrollMetrics(thread, 1200, 400, 120);
    act(() => {
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    Object.defineProperty(thread, "scrollHeight", { configurable: true, value: 1400 });
    act(() => {
      view.rerender({ rev: 1, key: "s1", historyScroll: undefined });
    });

    expect(thread.scrollTop).toBe(120);
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

    stubScrollMetrics(thread, 1200, 400, 180);
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

    expect(thread.scrollTop).toBe(480);
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

    expect(thread.scrollTop).toBe(200);
  });

  it("does not pin to live edge while a prepend restore is pending", () => {
    const pinSpy = vi.spyOn(sessionViewport, "pinTranscriptToLiveEdge");
    const { view, thread } = mountScroll("s1", 0, {
      hasEarlier: true,
      revealEarlier: () => 2,
      windowGeneration: 0,
    });

    stubScrollMetrics(thread, 1200, 400, liveEdgeScrollTop(1200, 400));
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

  it("auto-loads earlier rows near the top when armed", () => {
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

    stubScrollMetrics(thread, 1200, 400, 500);
    act(() => {
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    stubScrollMetrics(thread, 1200, 400, HISTORY_PRELOAD_PX);
    act(() => {
      vi.setSystemTime(AUTO_LOAD_COOLDOWN_MS + 10);
      view.result.current.onThreadScroll({ currentTarget: thread } as UIEvent<HTMLDivElement>);
    });

    expect(revealEarlier).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });
});
