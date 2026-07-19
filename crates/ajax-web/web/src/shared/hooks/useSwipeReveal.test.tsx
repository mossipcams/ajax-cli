import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
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
}: {
  onOffset?: (offset: number) => void;
  onOpenChange?: (open: boolean) => void;
}) {
  const ref = useRef<HTMLButtonElement>(null);
  useSwipeReveal(ref, { onOffset, onOpenChange });
  return <button ref={ref} type="button" data-testid="swipe-reveal-target" />;
}

describe("useSwipeReveal", () => {
  it("reports a settled-open offset after a horizontal left swipe", () => {
    const onOffset = vi.fn();
    const onOpenChange = vi.fn();
    render(<Harness onOffset={onOffset} onOpenChange={onOpenChange} />);
    const node = screen.getByTestId("swipe-reveal-target");

    node.dispatchEvent(touch("touchstart", 200, 100));
    node.dispatchEvent(touch("touchmove", 80, 100));
    node.dispatchEvent(new Event("touchend"));

    expect(onOpenChange).toHaveBeenLastCalledWith(true);
    expect(onOffset).toHaveBeenLastCalledWith(SWIPE_REVEAL_WIDTH);
  });

  it("settles closed when the swipe is mostly vertical", () => {
    const onOpenChange = vi.fn();
    const onOffset = vi.fn();
    render(<Harness onOffset={onOffset} onOpenChange={onOpenChange} />);
    const node = screen.getByTestId("swipe-reveal-target");

    node.dispatchEvent(touch("touchstart", 200, 100));
    node.dispatchEvent(touch("touchmove", 180, 260));
    node.dispatchEvent(new Event("touchend"));

    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(onOffset).toHaveBeenLastCalledWith(0);
  });
});
