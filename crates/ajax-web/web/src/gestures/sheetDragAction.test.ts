import { describe, it, expect, vi } from "vitest";
import { sheetDrag } from "./sheetDragAction";

function touch(type: string, clientY: number): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "touches", { value: [{ clientY }] });
  return event;
}

describe("sheetDrag action", () => {
  it("dismisses after a downward drag past the threshold", () => {
    const node = document.createElement("div");
    const onDismiss = vi.fn();
    sheetDrag(node, { onDismiss });

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 200));
    node.dispatchEvent(new Event("touchend"));

    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("springs back (no dismiss) on a small drag and resets offset", () => {
    const node = document.createElement("div");
    const onDismiss = vi.fn();
    const offsets: number[] = [];
    sheetDrag(node, { onDismiss, onOffset: (o) => offsets.push(o) });

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 20));
    node.dispatchEvent(new Event("touchend"));

    expect(onDismiss).not.toHaveBeenCalled();
    expect(offsets.at(-1)).toBe(0);
  });
});
