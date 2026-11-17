import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { useRef } from "react";
import { useSheetDrag } from "./useSheetDrag";

type Frame = { id: number; cb: FrameRequestCallback };
let frames: Frame[] = [];
let nextFrameId = 1;

function flushFrames() {
  const pending = frames;
  frames = [];
  act(() => {
    for (const frame of pending) frame.cb(0);
  });
}

beforeEach(() => {
  frames = [];
  nextFrameId = 1;
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    const id = nextFrameId++;
    frames.push({ id, cb });
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    frames = frames.filter((frame) => frame.id !== id);
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function touch(type: string, clientY: number, timeStamp: number): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "touches", { value: [{ clientY }] });
  Object.defineProperty(event, "timeStamp", { value: timeStamp });
  return event;
}

function ended(timeStamp: number): Event {
  const event = new Event("touchend", { bubbles: true });
  Object.defineProperty(event, "timeStamp", { value: timeStamp });
  return event;
}

function Harness({ onDismiss }: { onDismiss?: () => void }) {
  const grabRef = useRef<HTMLDivElement>(null);
  const targetRef = useRef<HTMLDivElement>(null);
  useSheetDrag(grabRef, { onDismiss: onDismiss ?? (() => {}), target: targetRef });
  return (
    <div ref={targetRef} data-testid="sheet-drag-card">
      <div ref={grabRef} data-testid="sheet-drag-target" />
    </div>
  );
}

describe("useSheetDrag", () => {
  it("dismisses after a downward drag past the threshold", () => {
    const onDismiss = vi.fn();
    render(<Harness onDismiss={onDismiss} />);
    const node = screen.getByTestId("sheet-drag-target");

    node.dispatchEvent(touch("touchstart", 0, 1000));
    node.dispatchEvent(touch("touchmove", 200, 1100));
    node.dispatchEvent(ended(1100));

    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("springs back (no dismiss) on a small drag and resets the transform", () => {
    const onDismiss = vi.fn();
    render(<Harness onDismiss={onDismiss} />);
    const node = screen.getByTestId("sheet-drag-target");
    const card = screen.getByTestId("sheet-drag-card");

    node.dispatchEvent(touch("touchstart", 0, 1000));
    node.dispatchEvent(touch("touchmove", 20, 1100));
    flushFrames();
    node.dispatchEvent(ended(1100));

    expect(onDismiss).not.toHaveBeenCalled();
    expect(card.style.transform).toBe("translateY(0px)");
  });

  it("#1656 coalesces touchmoves into one frame write of the latest offset", () => {
    render(<Harness />);
    const node = screen.getByTestId("sheet-drag-target");
    const card = screen.getByTestId("sheet-drag-card");

    node.dispatchEvent(touch("touchstart", 0, 1000));
    node.dispatchEvent(touch("touchmove", 10, 1010));
    node.dispatchEvent(touch("touchmove", 20, 1020));
    node.dispatchEvent(touch("touchmove", 30, 1030));

    expect(frames).toHaveLength(1);
    expect(card.style.transform).toBe("");

    flushFrames();
    expect(card.style.transform).toBe("translateY(30px)");
  });

  it("#1656 marks the card dragging so no CSS transition runs while the finger is down", () => {
    render(<Harness />);
    const node = screen.getByTestId("sheet-drag-target");
    const card = screen.getByTestId("sheet-drag-card");

    node.dispatchEvent(touch("touchstart", 0, 1000));
    node.dispatchEvent(touch("touchmove", 30, 1030));
    flushFrames();

    expect(card.classList.contains("is-dragging")).toBe(true);
  });

  it("#1656 cancels a pending frame on release", () => {
    render(<Harness />);
    const node = screen.getByTestId("sheet-drag-target");

    node.dispatchEvent(touch("touchstart", 0, 1000));
    node.dispatchEvent(touch("touchmove", 30, 1030));
    expect(frames).toHaveLength(1);

    node.dispatchEvent(ended(1040));
    expect(frames).toHaveLength(0);
  });

  it("#1616 dismisses a short fast downward flick below the distance threshold", () => {
    const onDismiss = vi.fn();
    render(<Harness onDismiss={onDismiss} />);
    const node = screen.getByTestId("sheet-drag-target");

    node.dispatchEvent(touch("touchstart", 0, 1000));
    node.dispatchEvent(touch("touchmove", 30, 1040));
    node.dispatchEvent(ended(1040));

    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("#1616 snaps back a slow drag below the distance threshold", () => {
    const onDismiss = vi.fn();
    render(<Harness onDismiss={onDismiss} />);
    const node = screen.getByTestId("sheet-drag-target");
    const card = screen.getByTestId("sheet-drag-card");

    node.dispatchEvent(touch("touchstart", 0, 1000));
    node.dispatchEvent(touch("touchmove", 10, 1100));
    node.dispatchEvent(touch("touchmove", 20, 1200));
    node.dispatchEvent(touch("touchmove", 30, 1300));
    node.dispatchEvent(touch("touchmove", 40, 1400));
    node.dispatchEvent(ended(1400));

    expect(onDismiss).not.toHaveBeenCalled();
    expect(card.style.transform).toBe("translateY(0px)");
  });

  it("#1616 rubber-bands an upward drag of 100px and springs back on release", () => {
    const onDismiss = vi.fn();
    render(<Harness onDismiss={onDismiss} />);
    const node = screen.getByTestId("sheet-drag-target");
    const card = screen.getByTestId("sheet-drag-card");

    node.dispatchEvent(touch("touchstart", 200, 1000));
    node.dispatchEvent(touch("touchmove", 100, 1050));
    flushFrames();

    const offset = Number(/translateY\((-?[\d.]+)px\)/.exec(card.style.transform)?.[1]);
    expect(offset).toBeLessThan(0);
    expect(Math.abs(offset)).toBeGreaterThan(0);
    expect(Math.abs(offset)).toBeLessThan(100);

    node.dispatchEvent(ended(1050));
    expect(onDismiss).not.toHaveBeenCalled();
    expect(card.style.transform).toBe("translateY(0px)");
  });

  it("cancels a pending frame on unmount", () => {
    const { unmount } = render(<Harness />);
    const node = screen.getByTestId("sheet-drag-target");

    node.dispatchEvent(touch("touchstart", 0, 1000));
    node.dispatchEvent(touch("touchmove", 30, 1030));
    expect(frames).toHaveLength(1);

    unmount();
    expect(frames).toHaveLength(0);
  });
});
