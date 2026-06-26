import { describe, it, expect, vi } from "vitest";
import { pullToRefresh } from "./pullToRefreshAction";

function touch(type: string, clientY: number): Event {
  const event = new Event(type, { bubbles: true });
  // jsdom lacks TouchEvent; attach the single field the action reads.
  Object.defineProperty(event, "touches", { value: [{ clientY }] });
  return event;
}

describe("pullToRefresh action", () => {
  it("fires onRefresh after an armed pull is released", () => {
    const node = document.createElement("div");
    const onRefresh = vi.fn();
    pullToRefresh(node, { onRefresh, scrollTop: () => 0 });

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 400));
    node.dispatchEvent(new Event("touchend"));

    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it("does not fire when the pull never passes the threshold", () => {
    const node = document.createElement("div");
    const onRefresh = vi.fn();
    pullToRefresh(node, { onRefresh, scrollTop: () => 0 });

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 10));
    node.dispatchEvent(new Event("touchend"));

    expect(onRefresh).not.toHaveBeenCalled();
  });

  it("does not activate when the container is scrolled down", () => {
    const node = document.createElement("div");
    const onRefresh = vi.fn();
    pullToRefresh(node, { onRefresh, scrollTop: () => 200 });

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 400));
    node.dispatchEvent(new Event("touchend"));

    expect(onRefresh).not.toHaveBeenCalled();
  });

  it("reports pull distance and resets it on release", () => {
    const node = document.createElement("div");
    const distances: number[] = [];
    pullToRefresh(node, { onRefresh: () => {}, scrollTop: () => 0, onDistance: (d) => distances.push(d) });

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 400));
    expect(distances.at(-1)).toBeGreaterThan(0);
    node.dispatchEvent(new Event("touchend"));
    expect(distances.at(-1)).toBe(0);
  });
});
