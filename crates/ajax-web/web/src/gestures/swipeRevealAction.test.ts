import { describe, it, expect, vi } from "vitest";
import { swipeReveal } from "./swipeRevealAction";
import { SWIPE_REVEAL_WIDTH } from "./swipeReveal";

function touch(type: string, clientX: number, clientY: number): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "touches", { value: [{ clientX, clientY }] });
  return event;
}

describe("swipeReveal action", () => {
  it("reports a settled-open offset after a horizontal left swipe", () => {
    const node = document.createElement("div");
    const onOffset = vi.fn();
    const onOpenChange = vi.fn();
    swipeReveal(node, { onOffset, onOpenChange });

    node.dispatchEvent(touch("touchstart", 200, 100));
    node.dispatchEvent(touch("touchmove", 80, 100));
    node.dispatchEvent(new Event("touchend"));

    expect(onOpenChange).toHaveBeenLastCalledWith(true);
    expect(onOffset).toHaveBeenLastCalledWith(SWIPE_REVEAL_WIDTH);
  });

  it("settles closed when the swipe is mostly vertical", () => {
    const node = document.createElement("div");
    const onOpenChange = vi.fn();
    const onOffset = vi.fn();
    swipeReveal(node, { onOffset, onOpenChange });

    node.dispatchEvent(touch("touchstart", 200, 100));
    node.dispatchEvent(touch("touchmove", 180, 260));
    node.dispatchEvent(new Event("touchend"));

    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(onOffset).toHaveBeenLastCalledWith(0);
  });
});
