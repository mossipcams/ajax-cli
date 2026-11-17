import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { useRef } from "react";
import { useSwipeReveal } from "./useSwipeReveal";
import { SWIPE_REVEAL_WIDTH } from "@/shared/gestures/swipeReveal";

function touch(type: string, clientX: number, clientY: number): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "touches", { value: [{ clientX, clientY }] });
  return event;
}

const frames = new Map<number, FrameRequestCallback>();
let nextFrameId = 1;

function flushFrames() {
  const pending = [...frames.values()];
  frames.clear();
  pending.forEach((callback) => callback(0));
}

function Harness({
  onOffset,
  onOpenChange,
  onDragChange,
  initialOffset = 0,
  ignoreSelector,
}: {
  onOffset?: (offset: number) => void;
  onOpenChange?: (open: boolean) => void;
  onDragChange?: (dragging: boolean) => void;
  initialOffset?: number;
  ignoreSelector?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const rowRef = useRef<HTMLButtonElement>(null);
  useSwipeReveal(ref, {
    onOffset,
    onOpenChange,
    onDragChange,
    getInitialOffset: () => initialOffset,
    ignoreSelector,
    target: rowRef,
  });
  return (
    <div ref={ref} data-testid="swipe-reveal-target">
      <div className="task-row-reveal" data-testid="swipe-reveal-action">
        <button type="button">Fix CI</button>
      </div>
      <button ref={rowRef} type="button" data-testid="swipe-reveal-row">
        Row
      </button>
    </div>
  );
}

describe("useSwipeReveal", () => {
  beforeEach(() => {
    frames.clear();
    nextFrameId = 1;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      const id = nextFrameId++;
      frames.set(id, callback);
      return id;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => {
      frames.delete(id);
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

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

  it("reports dragging from touchstart until touchend so the row transform is not transitioned mid-drag", () => {
    const onDragChange = vi.fn();
    render(<Harness onDragChange={onDragChange} />);
    const row = screen.getByTestId("swipe-reveal-row");

    row.dispatchEvent(touch("touchstart", 200, 100));
    row.dispatchEvent(touch("touchmove", 150, 100));
    row.dispatchEvent(touch("touchmove", 100, 100));
    expect(onDragChange.mock.calls).toEqual([[true]]);
    row.dispatchEvent(new Event("touchend", { bubbles: true }));
    expect(onDragChange.mock.calls).toEqual([[true], [false]]);
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

  it("#1648 writes the engaged track offset to the row once per frame and commits no state mid-drag", () => {
    const onOffset = vi.fn();
    const onOpenChange = vi.fn();
    render(<Harness onOffset={onOffset} onOpenChange={onOpenChange} />);
    const row = screen.getByTestId("swipe-reveal-row");

    row.dispatchEvent(touch("touchstart", 200, 100));
    row.dispatchEvent(touch("touchmove", 190, 100));
    row.dispatchEvent(touch("touchmove", 150, 100));
    row.dispatchEvent(touch("touchmove", 120, 100));
    expect(onOffset).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(frames.size).toBe(1);
    expect(row.style.transform).toBe("");

    flushFrames();
    expect(row.style.transform).toBe("translateX(-72px)");
    expect(onOffset).not.toHaveBeenCalled();
  });

  it("#1648 default-prevents vertical moves once the swipe has engaged but not before", () => {
    render(<Harness />);
    const row = screen.getByTestId("swipe-reveal-row");

    row.dispatchEvent(touch("touchstart", 200, 100));
    const unengaged = touch("touchmove", 200, 180);
    row.dispatchEvent(unengaged);
    expect(unengaged.defaultPrevented).toBe(false);
    row.dispatchEvent(new Event("touchend", { bubbles: true }));

    row.dispatchEvent(touch("touchstart", 200, 100));
    const engaged = touch("touchmove", 150, 104);
    row.dispatchEvent(engaged);
    expect(engaged.defaultPrevented).toBe(true);
    row.dispatchEvent(new Event("touchend", { bubbles: true }));
  });

  it("#1648 starts the engaged offset at the engage point so the first engaged frame does not jump", () => {
    render(<Harness />);
    const row = screen.getByTestId("swipe-reveal-row");

    row.dispatchEvent(touch("touchstart", 200, 100));
    row.dispatchEvent(touch("touchmove", 190, 100));
    flushFrames();
    expect(row.style.transform).toBe("translateX(-2px)");
    row.dispatchEvent(new Event("touchend", { bubbles: true }));
  });

  it("#1648 touchcancel restores the pre-drag offset without committing a new state", () => {
    const onOffset = vi.fn();
    const onOpenChange = vi.fn();
    render(
      <Harness onOffset={onOffset} onOpenChange={onOpenChange} initialOffset={SWIPE_REVEAL_WIDTH} />,
    );
    const row = screen.getByTestId("swipe-reveal-row");

    row.dispatchEvent(touch("touchstart", 200, 100));
    row.dispatchEvent(touch("touchmove", 280, 100));
    flushFrames();
    expect(row.style.transform).toBe("translateX(-86px)");
    row.dispatchEvent(new Event("touchcancel", { bubbles: true }));

    expect(onOffset).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(row.style.transform).toBe(`translateX(-${SWIPE_REVEAL_WIDTH}px)`);
    expect(frames.size).toBe(0);
  });
});
