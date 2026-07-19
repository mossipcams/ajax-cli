import { describe, it, expect, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { usePullToRefresh } from "./usePullToRefresh";
import { PULL_THRESHOLD } from "@/shared/gestures/pullToRefresh";

function touch(type: string, clientY: number): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "touches", { value: [{ clientY }] });
  return event;
}

describe("usePullToRefresh", () => {
  it("reports resisted distance and fires onRefresh when pull passes threshold", () => {
    const onRefresh = vi.fn();
    const onDistance = vi.fn();
    const { result } = renderHook(() =>
      usePullToRefresh({ onRefresh, onDistance, scrollTop: () => 0 }),
    );

    const node = document.createElement("div");
    result.current(node);

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", PULL_THRESHOLD * 4));
    expect(onDistance).toHaveBeenCalled();
    const lastDistance = onDistance.mock.calls.at(-1)?.[0] as number;
    expect(lastDistance).toBeGreaterThanOrEqual(PULL_THRESHOLD);

    node.dispatchEvent(new Event("touchend"));
    expect(onRefresh).toHaveBeenCalledTimes(1);
    expect(onDistance).toHaveBeenLastCalledWith(0);
  });

  it("does not fire onRefresh when the drag stays below threshold", () => {
    const onRefresh = vi.fn();
    const { result } = renderHook(() =>
      usePullToRefresh({ onRefresh, scrollTop: () => 0 }),
    );

    const node = document.createElement("div");
    result.current(node);

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 10));
    node.dispatchEvent(new Event("touchend"));

    expect(onRefresh).not.toHaveBeenCalled();
  });

  it("removes listeners when the ref is cleared", () => {
    const onRefresh = vi.fn();
    const { result } = renderHook(() =>
      usePullToRefresh({ onRefresh, scrollTop: () => 0 }),
    );

    const node = document.createElement("div");
    result.current(node);
    result.current(null);

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", PULL_THRESHOLD * 4));
    node.dispatchEvent(new Event("touchend"));

    expect(onRefresh).not.toHaveBeenCalled();
  });
});
