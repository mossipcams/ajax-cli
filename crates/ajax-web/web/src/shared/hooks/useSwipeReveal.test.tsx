import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { useRef } from "react";
import { useSwipeReveal } from "./useSwipeReveal";
import { SWIPE_REVEAL_WIDTH } from "@/shared/gestures/swipeReveal";

function touch(type: string, clientX: number, clientY: number): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "touches", { value: [{ clientX, clientY }] });
  return event;
}

function Harness({
  onOffset,
  onOpenChange,
  initialOffset = 0,
  ignoreSelector,
}: {
  onOffset?: (offset: number) => void;
  onOpenChange?: (open: boolean) => void;
  initialOffset?: number;
  ignoreSelector?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useSwipeReveal(ref, {
    onOffset,
    onOpenChange,
    getInitialOffset: () => initialOffset,
    ignoreSelector,
  });
  return (
    <div ref={ref} data-testid="swipe-reveal-target">
      <div className="task-row-reveal" data-testid="swipe-reveal-action">
        <button type="button">Fix CI</button>
      </div>
      <button type="button" data-testid="swipe-reveal-row">
        Row
      </button>
    </div>
  );
}

describe("useSwipeReveal", () => {
  it("reports a settled-open offset after a horizontal left swipe on the row", () => {
    const onOffset = vi.fn();
    const onOpenChange = vi.fn();
    render(<Harness onOffset={onOffset} onOpenChange={onOpenChange} />);
    const row = screen.getByTestId("swipe-reveal-row");

    row.dispatchEvent(touch("touchstart", 200, 100));
    row.dispatchEvent(touch("touchmove", 80, 100));
    row.dispatchEvent(new Event("touchend", { bubbles: true }));

    expect(onOpenChange).toHaveBeenLastCalledWith(true);
    expect(onOffset).toHaveBeenLastCalledWith(SWIPE_REVEAL_WIDTH);
  });

  it("settles closed when the swipe is mostly vertical", () => {
    const onOpenChange = vi.fn();
    const onOffset = vi.fn();
    render(<Harness onOffset={onOffset} onOpenChange={onOpenChange} />);
    const row = screen.getByTestId("swipe-reveal-row");

    row.dispatchEvent(touch("touchstart", 200, 100));
    row.dispatchEvent(touch("touchmove", 180, 260));
    row.dispatchEvent(new Event("touchend", { bubbles: true }));

    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(onOffset).toHaveBeenLastCalledWith(0);
  });

  it("settles closed after a rightward swipe from an open offset on the wrap", () => {
    const onOpenChange = vi.fn();
    const onOffset = vi.fn();
    render(<Harness onOffset={onOffset} onOpenChange={onOpenChange} initialOffset={SWIPE_REVEAL_WIDTH} />);
    const wrap = screen.getByTestId("swipe-reveal-target");

    wrap.dispatchEvent(touch("touchstart", 200, 100));
    wrap.dispatchEvent(touch("touchmove", 380, 100));
    wrap.dispatchEvent(new Event("touchend", { bubbles: true }));

    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(onOffset).toHaveBeenLastCalledWith(0);
  });

  it("ignores touchstart inside the reveal layer so action taps do not start a swipe", () => {
    const onOffset = vi.fn();
    const onOpenChange = vi.fn();
    render(
      <Harness
        onOffset={onOffset}
        onOpenChange={onOpenChange}
        ignoreSelector=".task-row-reveal"
      />,
    );
    const action = screen.getByRole("button", { name: "Fix CI" });

    fireEvent.touchStart(action, { touches: [{ clientX: 200, clientY: 100 }] });
    fireEvent.touchMove(action, { touches: [{ clientX: 80, clientY: 100 }] });
    fireEvent.touchEnd(action, { changedTouches: [{ clientX: 80, clientY: 100 }] });

    expect(onOpenChange).not.toHaveBeenCalled();
    expect(onOffset).not.toHaveBeenCalled();
  });
});
