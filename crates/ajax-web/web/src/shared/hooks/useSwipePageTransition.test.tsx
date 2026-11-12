import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { useRef } from "react";
import {
  PageCrossSlideProvider,
  useSwipePageTransition,
  usePageCrossSlide,
  SWIPE_PAGE_COMMIT_MS,
  CROSS_SLIDE_ARMED_SLACK_MS,
} from "./useSwipePageTransition";
import { gestureBusyGate } from "@/shared/lib/cockpitPoll";
import { taskHash } from "@/shared/lib/routes";
import {
  navigateHashBack,
  setSwipeEnterDirection,
} from "@/shared/lib/swipeEnter";
import {
  setTerminalDoubleTapPending,
  setTerminalSelecting,
} from "@/shared/lib/terminalSelecting";
import * as telemetry from "@/shared/lib/telemetry";

vi.mock("@/shared/lib/telemetry", async () => {
  const actual = await vi.importActual<typeof import("@/shared/lib/telemetry")>(
    "@/shared/lib/telemetry",
  );
  return {
    ...actual,
    captureSwipe: vi.fn(actual.captureSwipe),
    markNavigationStart: vi.fn(actual.markNavigationStart),
  };
});

vi.mock("@/shared/lib/swipeEnter", async () => {
  const actual = await vi.importActual<typeof import("@/shared/lib/swipeEnter")>(
    "@/shared/lib/swipeEnter",
  );
  return {
    ...actual,
    setSwipeEnterDirection: vi.fn(actual.setSwipeEnterDirection),
  };
});

function touch(
  type: string,
  clientX: number,
  clientY = 40,
  target?: EventTarget,
): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "touches", { value: [{ clientX, clientY }] });
  Object.defineProperty(event, "changedTouches", { value: [{ clientX, clientY }] });
  if (target) Object.defineProperty(event, "target", { value: target });
  return event;
}

function controlPerformanceNow(start: number) {
  let now = start;
  const spy = vi.spyOn(performance, "now").mockImplementation(() => now);
  return {
    set(ms: number) {
      now = ms;
    },
    restore() {
      spy.mockRestore();
    },
  };
}

function Harness({
  onLeft,
  onRight,
  commitRef,
  from_route,
}: {
  onLeft?: () => void;
  onRight?: () => void;
  commitRef?: { current: ((direction: "left" | "right") => void) | null };
  from_route?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const { style, commit } = useSwipePageTransition(ref, { onLeft, onRight, from_route });
  if (commitRef) commitRef.current = commit;
  return <div ref={ref} data-testid="swipe-target" style={style} />;
}

function renderHarness(props: Parameters<typeof Harness>[0] = {}) {
  return render(
    <PageCrossSlideProvider>
      <Harness {...props} />
    </PageCrossSlideProvider>,
  );
}

function LeavingPaneProbe() {
  const crossSlide = usePageCrossSlide();
  return <div data-testid="leaving-pane" style={crossSlide?.paneStyle("leaving")} />;
}

describe("useSwipePageTransition", () => {
  let clock: ReturnType<typeof controlPerformanceNow> | null = null;

  beforeEach(() => {
    vi.useFakeTimers();
    window.location.hash = taskHash("web/fix-login");
    while (gestureBusyGate.isBusy()) gestureBusyGate.end();
    vi.mocked(setSwipeEnterDirection).mockClear();
    vi.mocked(telemetry.captureSwipe).mockClear();
    vi.mocked(telemetry.markNavigationStart).mockClear();
  });

  afterEach(() => {
    clock?.restore();
    clock = null;
    vi.useRealTimers();
    window.location.hash = "";
    delete document.documentElement.dataset.ajaxTerminalSelecting;
    delete document.documentElement.dataset.ajaxTerminalDoubleTapPending;
  });

  it("commits left after the slide animation", async () => {
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    node.dispatchEvent(touch("touchmove", 120, 42, node));
    node.dispatchEvent(touch("touchmove", 80, 42, node));
    await act(async () => {
      node.dispatchEvent(touch("touchend", 80, 42, node));
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).toHaveBeenCalledOnce();
    expect(telemetry.captureSwipe).toHaveBeenCalledWith(
      expect.objectContaining({
        direction: "left",
        completed: true,
        cancelled: false,
        page_width_px: 390,
        settle_ms: expect.any(Number),
        distance_px: expect.any(Number),
      }),
    );
  });

  it("ignores sub-dead-zone drags without telemetry", async () => {
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    node.dispatchEvent(touch("touchmove", 180, 42, node));
    node.dispatchEvent(touch("touchend", 180, 42, node));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
    expect(telemetry.captureSwipe).not.toHaveBeenCalled();
  });

  it("springs back without navigation when engaged but under commit", async () => {
    clock = controlPerformanceNow(0);
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 300, 40, node));
    node.dispatchEvent(touch("touchmove", 250, 42, node));
    clock.set(1000);
    node.dispatchEvent(touch("touchmove", 100, 42, node));
    node.dispatchEvent(touch("touchend", 100, 42, node));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
    expect(telemetry.captureSwipe).toHaveBeenCalledWith(
      expect.objectContaining({
        completed: false,
        cancelled: true,
        page_width_px: expect.any(Number),
        settle_ms: expect.any(Number),
      }),
    );
  });

  it("commits a slow drag past 50% of the page width", async () => {
    clock = controlPerformanceNow(0);
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 300, 40, node));
    node.dispatchEvent(touch("touchmove", 250, 42, node));
    clock.set(1000);
    node.dispatchEvent(touch("touchmove", 50, 42, node));
    node.dispatchEvent(touch("touchend", 50, 42, node));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).toHaveBeenCalledOnce();
  });

  it("commits a short fast flick", async () => {
    clock = controlPerformanceNow(0);
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 300, 40, node));
    node.dispatchEvent(touch("touchmove", 250, 42, node));
    clock.set(10);
    node.dispatchEvent(touch("touchmove", 200, 42, node));
    node.dispatchEvent(touch("touchend", 200, 42, node));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).toHaveBeenCalledOnce();
  });

  it("commits a swipe whose engaging move is also its last move", async () => {
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    node.dispatchEvent(touch("touchmove", 120, 42, node));
    node.dispatchEvent(touch("touchend", 120, 42, node));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).toHaveBeenCalledOnce();
  });

  it("springs back a release moving backward past 50% of the page width", async () => {
    clock = controlPerformanceNow(0);
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 300, 40, node));
    node.dispatchEvent(touch("touchmove", 250, 42, node));
    clock.set(1000);
    node.dispatchEvent(touch("touchmove", 40, 42, node));
    clock.set(1010);
    node.dispatchEvent(touch("touchmove", 50, 42, node));
    node.dispatchEvent(touch("touchend", 50, 42, node));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
  });

  it("engages a left-edge rightward swipe after 10px and tracks the finger 1:1", async () => {
    const onRight = vi.fn();
    renderHarness({ onRight });
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 40, 40, node));
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 49, 42, node));
    });
    expect(node.style.transform).toBe("");
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 50, 42, node));
    });
    expect(node.style.transform).toBe("");
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 70, 42, node));
    });
    expect(node.style.transform).toBe("translate3d(20px, 0, 0)");
  });

  it("keeps the 48px dead-zone for a mid-screen rightward drag", async () => {
    renderHarness({});
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 240, 42, node));
    });
    expect(node.style.transform).toBe("");
  });

  it("commits right programmatically after the slide animation", async () => {
    const onRight = vi.fn();
    const commitRef: { current: ((direction: "left" | "right") => void) | null } = {
      current: null,
    };
    renderHarness({ onRight, commitRef });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    commitRef.current!("right");
    expect(onRight).toHaveBeenCalledOnce();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onRight).toHaveBeenCalledOnce();
  });

  it("ignores a second programmatic commit while settling", async () => {
    const onRight = vi.fn();
    const commitRef: { current: ((direction: "left" | "right") => void) | null } = {
      current: null,
    };
    renderHarness({ onRight, commitRef });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    commitRef.current!("right");
    commitRef.current!("right");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onRight).toHaveBeenCalledOnce();
  });

  it("does not commit right when the list route is already active (#1064)", async () => {
    window.location.hash = "#/";
    const onRight = vi.fn();
    renderHarness({ onRight, from_route: taskHash("web/fix-login") });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    await act(async () => {
      node.dispatchEvent(touch("touchstart", 40, 40, node));
      node.dispatchEvent(touch("touchmove", 120, 42, node));
      node.dispatchEvent(touch("touchmove", 200, 42, node));
      node.dispatchEvent(touch("touchend", 200, 42, node));
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onRight).not.toHaveBeenCalled();
  });

  it("aborts an in-flight swipe when terminal text selecting becomes active", async () => {
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    setTerminalSelecting(true);
    node.dispatchEvent(touch("touchmove", 120, 42, node));
    node.dispatchEvent(touch("touchend", 120, 42, node));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
    setTerminalSelecting(false);
  });

  it("commits navigation before the swipe surface unmounts", async () => {
    const onLeft = vi.fn();
    const { unmount } = renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    node.dispatchEvent(touch("touchmove", 120, 42, node));
    node.dispatchEvent(touch("touchmove", 80, 42, node));
    node.dispatchEvent(touch("touchend", 80, 42, node));
    expect(onLeft).toHaveBeenCalledOnce();
    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).toHaveBeenCalledOnce();
  });

  it("does not arm swipe on terminal when a double-tap is pending", async () => {
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    const host = document.createElement("div");
    host.className = "terminal-host";
    node.appendChild(host);

    setTerminalDoubleTapPending(true);
    host.dispatchEvent(touch("touchstart", 200, 40, host));
    host.dispatchEvent(touch("touchmove", 120, 42, host));
    host.dispatchEvent(touch("touchend", 120, 42, host));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
    setTerminalDoubleTapPending(false);
  });

  it.each(["session-tool-output", "session-diff-body", "md-block", "md-table-wrap"])(
    "leaves touch scrolling to %s bodies (#1153)",
    async (className) => {
      const onLeft = vi.fn();
      renderHarness({ onLeft });
      const node = screen.getByTestId("swipe-target");
      const body = document.createElement("pre");
      body.className = className;
      node.appendChild(body);

      body.dispatchEvent(touch("touchstart", 200, 200, body));
      const move = touch("touchmove", 80, 120, body);
      body.dispatchEvent(move);
      body.dispatchEvent(touch("touchend", 80, 120, body));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
      });

      expect(move.defaultPrevented).toBe(false);
      expect(onLeft).not.toHaveBeenCalled();
      expect(node.style.transform).toBe("");
    },
  );

  it("settles and reattaches swipe listeners when animating flip is skipped (#1077)", async () => {
    const originalUserAgent = navigator.userAgent;
    const originalRaf = window.requestAnimationFrame;
    Object.defineProperty(navigator, "userAgent", {
      value: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)",
      configurable: true,
    });
    window.requestAnimationFrame = vi.fn(() => 0) as typeof requestAnimationFrame;

    clock = controlPerformanceNow(0);
    try {
      const onLeft = vi.fn();
      renderHarness({ onLeft });
      const node = screen.getByTestId("swipe-target");
      Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

      node.dispatchEvent(touch("touchstart", 300, 40, node));
      node.dispatchEvent(touch("touchmove", 220, 42, node));
      clock.set(1000);
      node.dispatchEvent(touch("touchmove", 20, 42, node));
      await act(async () => {
        node.dispatchEvent(touch("touchend", 20, 42, node));
      });
      expect(onLeft).toHaveBeenCalledOnce();
      expect(gestureBusyGate.isBusy()).toBe(true);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 40);
      });
      expect(gestureBusyGate.isBusy()).toBe(true);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(CROSS_SLIDE_ARMED_SLACK_MS);
      });
      expect(gestureBusyGate.isBusy()).toBe(false);

      clock.set(2000);
      node.dispatchEvent(touch("touchstart", 300, 40, node));
      node.dispatchEvent(touch("touchmove", 220, 42, node));
      clock.set(3000);
      node.dispatchEvent(touch("touchmove", 20, 42, node));
      await act(async () => {
        node.dispatchEvent(touch("touchend", 20, 42, node));
        await vi.advanceTimersByTimeAsync(
          SWIPE_PAGE_COMMIT_MS + 40 + CROSS_SLIDE_ARMED_SLACK_MS,
        );
      });
      expect(onLeft).toHaveBeenCalledTimes(2);
    } finally {
      Object.defineProperty(navigator, "userAgent", {
        value: originalUserAgent,
        configurable: true,
      });
      window.requestAnimationFrame = originalRaf;
    }
  });

  it("holds the leaving pane until a swipe-back history traversal lands", async () => {
    history.replaceState({ ajaxPrevHash: "#/" }, "");
    const back = vi.spyOn(history, "back").mockImplementation(() => {});
    try {
      render(
        <PageCrossSlideProvider>
          <Harness onRight={() => navigateHashBack("#/")} />
          <LeavingPaneProbe />
        </PageCrossSlideProvider>,
      );
      const node = screen.getByTestId("swipe-target");
      Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

      await act(async () => {
        node.dispatchEvent(touch("touchstart", 40, 40, node));
        node.dispatchEvent(touch("touchmove", 160, 42, node));
        node.dispatchEvent(touch("touchmove", 200, 42, node));
        node.dispatchEvent(touch("touchend", 200, 42, node));
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(back).toHaveBeenCalledOnce();
      const pane = screen.getByTestId("leaving-pane");
      expect(pane.style.transition).toBe("none");

      await act(async () => {
        history.replaceState(null, "", "#/");
        window.dispatchEvent(new Event("hashchange"));
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(pane.style.transform).toBe("translate3d(390px, 0, 0)");
      expect(pane.style.transition).toContain("transform");
    } finally {
      back.mockRestore();
    }
  });
});
