import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { useRef } from "react";
import { useSwipePageTransition, SWIPE_PAGE_COMMIT_MS } from "./useSwipePageTransition";
import { setSwipeEnterDirection } from "@/shared/lib/swipeEnter";

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
}: {
  onLeft?: () => void;
  onRight?: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const { style } = useSwipePageTransition(ref, { onLeft, onRight });
  return <div ref={ref} data-testid="swipe-target" style={style} />;
}

describe("useSwipePageTransition", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(setSwipeEnterDirection).mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
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
  });

  it("springs back without navigation on a short drag", async () => {
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
  });
});
