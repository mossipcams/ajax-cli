import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { useRef } from "react";
import { useSwipePageTransition, SWIPE_PAGE_COMMIT_MS } from "./useSwipePageTransition";
import { setSwipeEnterDirection } from "@/shared/lib/swipeEnter";
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

function Harness({
  onLeft,
  onRight,
  commitRef,
}: {
  onLeft?: () => void;
  onRight?: () => void;
  commitRef?: { current: ((direction: "left" | "right") => void) | null };
}) {
  const ref = useRef<HTMLDivElement>(null);
  const { style, commit } = useSwipePageTransition(ref, { onLeft, onRight });
  if (commitRef) commitRef.current = commit;
  return <div ref={ref} data-testid="swipe-target" style={style} />;
}

describe("useSwipePageTransition", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(setSwipeEnterDirection).mockClear();
    vi.mocked(telemetry.captureSwipe).mockClear();
    vi.mocked(telemetry.markNavigationStart).mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
    delete document.documentElement.dataset.ajaxTerminalSelecting;
    delete document.documentElement.dataset.ajaxTerminalDoubleTapPending;
  });

  it("commits left after the slide animation", async () => {
    const onLeft = vi.fn();
    render(<Harness onLeft={onLeft} />);
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    node.dispatchEvent(touch("touchmove", 120, 42, node));
    node.dispatchEvent(touch("touchend", 120, 42, node));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(setSwipeEnterDirection).toHaveBeenCalledWith("left");
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
    render(<Harness onLeft={onLeft} />);
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    node.dispatchEvent(touch("touchmove", 180, 42, node));
    node.dispatchEvent(touch("touchend", 180, 42, node));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(setSwipeEnterDirection).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
    expect(telemetry.captureSwipe).not.toHaveBeenCalled();
  });

  it("springs back without navigation when engaged but under commit", async () => {
    const onLeft = vi.fn();
    render(<Harness onLeft={onLeft} />);
    const node = screen.getByTestId("swipe-target");

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    node.dispatchEvent(touch("touchmove", 150, 42, node));
    node.dispatchEvent(touch("touchend", 150, 42, node));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(setSwipeEnterDirection).not.toHaveBeenCalled();
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

  it("commits right programmatically after the slide animation", async () => {
    const onRight = vi.fn();
    const commitRef: { current: ((direction: "left" | "right") => void) | null } = {
      current: null,
    };
    render(<Harness onRight={onRight} commitRef={commitRef} />);
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    commitRef.current!("right");
    expect(onRight).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(setSwipeEnterDirection).toHaveBeenCalledWith("right");
    expect(onRight).toHaveBeenCalledOnce();
  });

  it("ignores a second programmatic commit while settling", async () => {
    const onRight = vi.fn();
    const commitRef: { current: ((direction: "left" | "right") => void) | null } = {
      current: null,
    };
    render(<Harness onRight={onRight} commitRef={commitRef} />);
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    commitRef.current!("right");
    commitRef.current!("right");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onRight).toHaveBeenCalledOnce();
  });

  it("aborts an in-flight swipe when terminal text selecting becomes active", async () => {
    const onLeft = vi.fn();
    render(<Harness onLeft={onLeft} />);
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
    expect(setSwipeEnterDirection).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
    setTerminalSelecting(false);
  });

  it("aborts settle on unmount without swipe or nav mark side effects", async () => {
    const onLeft = vi.fn();
    const { unmount } = render(<Harness onLeft={onLeft} />);
    const node = screen.getByTestId("swipe-target");
    Object.defineProperty(node, "clientWidth", { value: 390, configurable: true });

    node.dispatchEvent(touch("touchstart", 200, 40, node));
    node.dispatchEvent(touch("touchmove", 120, 42, node));
    node.dispatchEvent(touch("touchend", 120, 42, node));
    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onLeft).not.toHaveBeenCalled();
    expect(setSwipeEnterDirection).not.toHaveBeenCalled();
    expect(telemetry.captureSwipe).not.toHaveBeenCalled();
    expect(telemetry.markNavigationStart).not.toHaveBeenCalled();
  });

  it("does not arm swipe on terminal when a double-tap is pending", async () => {
    const onLeft = vi.fn();
    render(<Harness onLeft={onLeft} />);
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
    expect(setSwipeEnterDirection).not.toHaveBeenCalled();
    expect(node.style.transform).toBe("");
    setTerminalDoubleTapPending(false);
  });
});
