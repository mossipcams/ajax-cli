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
  timeStamp?: number,
  fingers = 1,
): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  const points = Array.from({ length: fingers }, () => ({ clientX, clientY }));
  Object.defineProperty(event, "touches", { value: points });
  Object.defineProperty(event, "changedTouches", { value: [{ clientX, clientY }] });
  if (target) Object.defineProperty(event, "target", { value: target });
  if (timeStamp !== undefined) Object.defineProperty(event, "timeStamp", { value: timeStamp });
  return event;
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

function BelowActiveProbe() {
  const crossSlide = usePageCrossSlide();
  return (
    <div data-testid="below-active" data-below={crossSlide?.belowActive ? "true" : "false"} />
  );
}

function renderWithBelowProbe(props: Parameters<typeof Harness>[0] = {}) {
  return render(
    <PageCrossSlideProvider>
      <Harness {...props} />
      <BelowActiveProbe />
    </PageCrossSlideProvider>,
  );
}

function belowMounted(): boolean {
  return screen.getByTestId("below-active").dataset.below === "true";
}

describe("useSwipePageTransition", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    window.location.hash = taskHash("web/fix-login");
    while (gestureBusyGate.isBusy()) gestureBusyGate.end();
    vi.mocked(setSwipeEnterDirection).mockClear();
    vi.mocked(telemetry.captureSwipe).mockClear();
    vi.mocked(telemetry.markNavigationStart).mockClear();
  });

  afterEach(() => {
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
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 300, 40, node, 1000));
    node.dispatchEvent(touch("touchmove", 250, 42, node, 1000));
    node.dispatchEvent(touch("touchmove", 100, 42, node, 2000));
    node.dispatchEvent(touch("touchend", 100, 42, node, 2000));

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
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 300, 40, node, 1000));
    node.dispatchEvent(touch("touchmove", 250, 42, node, 1000));
    node.dispatchEvent(touch("touchmove", 50, 42, node, 2000));
    node.dispatchEvent(touch("touchend", 50, 42, node, 2000));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).toHaveBeenCalledOnce();
  });

  it("commits a short fast flick", async () => {
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 300, 40, node, 1000));
    node.dispatchEvent(touch("touchmove", 250, 42, node, 1000));
    node.dispatchEvent(touch("touchmove", 200, 42, node, 1010));
    node.dispatchEvent(touch("touchend", 200, 42, node, 1010));
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
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 300, 40, node, 1000));
    node.dispatchEvent(touch("touchmove", 250, 42, node, 1000));
    node.dispatchEvent(touch("touchmove", 40, 42, node, 2000));
    node.dispatchEvent(touch("touchmove", 50, 42, node, 2010));
    node.dispatchEvent(touch("touchend", 50, 42, node, 2010));
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

    try {
      const onLeft = vi.fn();
      renderHarness({ onLeft });
      const node = screen.getByTestId("swipe-target");
      Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

      node.dispatchEvent(touch("touchstart", 300, 40, node, 1000));
      node.dispatchEvent(touch("touchmove", 220, 42, node, 1000));
      node.dispatchEvent(touch("touchmove", 20, 42, node, 2000));
      await act(async () => {
        node.dispatchEvent(touch("touchend", 20, 42, node, 2000));
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

      node.dispatchEvent(touch("touchstart", 300, 40, node, 3000));
      node.dispatchEvent(touch("touchmove", 220, 42, node, 3000));
      node.dispatchEvent(touch("touchmove", 20, 42, node, 4000));
      await act(async () => {
        node.dispatchEvent(touch("touchend", 20, 42, node, 4000));
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
  it("cancels a mid-screen rightward swipe flicked back past its origin", async () => {
    const onRight = vi.fn();
    renderHarness({ onRight });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 200, 40, node, 1000));
    node.dispatchEvent(touch("touchmove", 260, 42, node, 1000));
    node.dispatchEvent(touch("touchmove", 100, 42, node, 1010));
    node.dispatchEvent(touch("touchend", 100, 42, node, 1010));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onRight).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
  });

  it("cancels an edge-armed rightward swipe flicked back past its origin", async () => {
    const onRight = vi.fn();
    renderHarness({ onRight });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 40, 40, node, 1000));
    node.dispatchEvent(touch("touchmove", 60, 42, node, 1000));
    node.dispatchEvent(touch("touchmove", 10, 42, node, 1010));
    node.dispatchEvent(touch("touchend", 10, 42, node, 1010));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onRight).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
  });

  it("cancels the gesture when a second finger lands", async () => {
    const onLeft = vi.fn();
    renderHarness({ onLeft });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 200, 40, node, 1000));
    node.dispatchEvent(touch("touchmove", 120, 42, node, 1000));
    node.dispatchEvent(touch("touchstart", 150, 60, node, 1005, 2));
    node.dispatchEvent(touch("touchmove", 60, 42, node, 1010, 2));
    node.dispatchEvent(touch("touchend", 60, 42, node, 1010));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
  });

  it("measures the edge zone from the root's left edge", async () => {
    renderHarness({});
    const node = screen.getByTestId("swipe-target");
    vi.spyOn(node, "getBoundingClientRect").mockReturnValue({ left: 100 } as DOMRect);

    node.dispatchEvent(touch("touchstart", 120, 40, node, 1000));
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 130, 42, node, 1000));
    });
    expect(node.style.transform).toBe("");
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 140, 42, node, 1000));
    });
    expect(node.style.transform).toBe("translate3d(10px, 0, 0)");
  });

  it("does not edge-arm a touch that sits beyond the root's edge zone", async () => {
    renderHarness({});
    const node = screen.getByTestId("swipe-target");
    vi.spyOn(node, "getBoundingClientRect").mockReturnValue({ left: 100 } as DOMRect);

    node.dispatchEvent(touch("touchstart", 200, 40, node, 1000));
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 210, 42, node, 1000));
    });
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 220, 42, node, 1000));
    });
    expect(node.style.transform).toBe("");
  });

  it("needs the full 48px to engage an edge touch on a textarea", async () => {
    renderHarness({});
    const node = screen.getByTestId("swipe-target");
    const field = document.createElement("textarea");
    node.appendChild(field);

    node.dispatchEvent(touch("touchstart", 40, 40, field, 1000));
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 50, 42, field, 1000));
    });
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 80, 42, field, 1000));
    });
    expect(node.style.transform).toBe("");
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 120, 42, field, 1000));
    });
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 130, 42, field, 1000));
    });
    expect(node.style.transform).toBe("translate3d(10px, 0, 0)");
  });

  it("needs the full 48px for an edge touch held 300ms before moving", async () => {
    renderHarness({});
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 40, 40, node, 1000));
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 50, 42, node, 1300));
    });
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 80, 42, node, 1300));
    });
    expect(node.style.transform).toBe("");
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 120, 42, node, 1300));
    });
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 130, 42, node, 1300));
    });
    expect(node.style.transform).toBe("translate3d(10px, 0, 0)");
  });

  it("mounts the below layer on an edge touchstart and unmounts on a no-move release", async () => {
    renderWithBelowProbe({ onRight: vi.fn() });
    const node = screen.getByTestId("swipe-target");

    expect(belowMounted()).toBe(false);
    await act(async () => {
      node.dispatchEvent(touch("touchstart", 40, 40, node));
    });
    expect(belowMounted()).toBe(true);
    await act(async () => {
      node.dispatchEvent(touch("touchend", 40, 40, node));
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(belowMounted()).toBe(false);
  });

  it("mounts the below layer only after a mid-screen rightward drag engages", async () => {
    renderWithBelowProbe({ onRight: vi.fn() });
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    expect(belowMounted()).toBe(false);
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 240, 42, node));
    });
    expect(belowMounted()).toBe(false);
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 250, 42, node));
    });
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 260, 42, node));
    });
    expect(belowMounted()).toBe(true);
  });

  it("keeps the below layer mounted during a spring-back until it settles", async () => {
    renderWithBelowProbe({ onRight: vi.fn() });
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 40, 40, node, 1000));
    node.dispatchEvent(touch("touchmove", 70, 42, node, 1000));
    node.dispatchEvent(touch("touchmove", 120, 42, node, 2000));
    await act(async () => {
      node.dispatchEvent(touch("touchend", 120, 42, node, 2000));
    });
    expect(belowMounted()).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(belowMounted()).toBe(false);
  });

  it("keeps the below layer mounted while a committed right cross-slide is active", async () => {
    const originalUserAgent = navigator.userAgent;
    const originalRaf = window.requestAnimationFrame;
    Object.defineProperty(navigator, "userAgent", {
      value: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)",
      configurable: true,
    });
    window.requestAnimationFrame = vi.fn(() => 0) as typeof requestAnimationFrame;

    try {
      const onRight = vi.fn();
      renderWithBelowProbe({ onRight });
      const node = screen.getByTestId("swipe-target");
      Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

      await act(async () => {
        node.dispatchEvent(touch("touchstart", 40, 40, node));
        node.dispatchEvent(touch("touchmove", 160, 42, node));
        node.dispatchEvent(touch("touchmove", 200, 42, node));
        node.dispatchEvent(touch("touchend", 200, 42, node));
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(onRight).toHaveBeenCalledOnce();
      expect(belowMounted()).toBe(true);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(
          SWIPE_PAGE_COMMIT_MS + 40 + CROSS_SLIDE_ARMED_SLACK_MS,
        );
      });
      expect(onRight).toHaveBeenCalledOnce();
      expect(belowMounted()).toBe(false);
    } finally {
      Object.defineProperty(navigator, "userAgent", {
        value: originalUserAgent,
        configurable: true,
      });
      window.requestAnimationFrame = originalRaf;
    }
  });

  it("never mounts the below layer for a leftward engaged drag", async () => {
    renderWithBelowProbe({ onLeft: vi.fn() });
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 300, 40, node, 1000));
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 250, 42, node, 1000));
    });
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 120, 42, node, 2000));
    });
    expect(belowMounted()).toBe(false);
    await act(async () => {
      node.dispatchEvent(touch("touchend", 120, 42, node, 2000));
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(belowMounted()).toBe(false);
  });

  it("does not mount the below layer on an edge touchstart while the list route is active (#1064)", async () => {
    window.location.hash = "#/";
    renderWithBelowProbe({ onRight: vi.fn(), from_route: taskHash("web/fix-login") });
    const node = screen.getByTestId("swipe-target");

    await act(async () => {
      node.dispatchEvent(touch("touchstart", 40, 40, node));
    });
    expect(belowMounted()).toBe(false);
    await act(async () => {
      node.dispatchEvent(touch("touchmove", 120, 42, node));
      node.dispatchEvent(touch("touchend", 120, 42, node));
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(belowMounted()).toBe(false);
  });
});
