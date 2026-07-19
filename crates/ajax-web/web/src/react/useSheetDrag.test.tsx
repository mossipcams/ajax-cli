import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { useRef } from "react";
import { useSheetDrag } from "./useSheetDrag";

function touch(type: string, clientY: number): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "touches", { value: [{ clientY }] });
  return event;
}

function Harness({
  onDismiss,
  onOffset,
}: {
  onDismiss?: () => void;
  onOffset?: (offset: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useSheetDrag(ref, { onDismiss: onDismiss ?? (() => {}), onOffset });
  return <div ref={ref} data-testid="sheet-drag-target" />;
}

describe("useSheetDrag", () => {
  it("dismisses after a downward drag past the threshold", () => {
    const onDismiss = vi.fn();
    render(<Harness onDismiss={onDismiss} />);
    const node = screen.getByTestId("sheet-drag-target");

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 200));
    node.dispatchEvent(new Event("touchend"));

    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("springs back (no dismiss) on a small drag and resets offset", () => {
    const onDismiss = vi.fn();
    const offsets: number[] = [];
    render(
      <Harness onDismiss={onDismiss} onOffset={(o) => offsets.push(o)} />,
    );
    const node = screen.getByTestId("sheet-drag-target");

    node.dispatchEvent(touch("touchstart", 0));
    node.dispatchEvent(touch("touchmove", 20));
    node.dispatchEvent(new Event("touchend"));

    expect(onDismiss).not.toHaveBeenCalled();
    expect(offsets.at(-1)).toBe(0);
  });
});
