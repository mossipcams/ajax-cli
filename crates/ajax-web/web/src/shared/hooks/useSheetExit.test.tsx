import type { CSSProperties } from "react";
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import { useSheetExit } from "./useSheetExit";

function Harness({ onClose, cardStyle }: { onClose: () => void; cardStyle?: CSSProperties }) {
  const { scrimRef, cardRef, requestClose } = useSheetExit(onClose);
  return (
    <div ref={scrimRef} data-testid="scrim">
      <div ref={cardRef} data-testid="card" style={cardStyle}>
        <button type="button" onClick={requestClose}>
          close
        </button>
      </div>
    </div>
  );
}

function stubAnimate() {
  let resolve!: () => void;
  const finished = new Promise<void>((r) => {
    resolve = r;
  });
  const animate = vi.fn(() => ({ finished }));
  Object.defineProperty(Element.prototype, "animate", {
    configurable: true,
    writable: true,
    value: animate,
  });
  return { animate, finish: () => resolve() };
}

function cardKeyframes(animate: ReturnType<typeof stubAnimate>["animate"]) {
  const calls = animate.mock.calls as unknown as [Keyframe[], KeyframeAnimationOptions][];
  return calls.find(([frames]) => frames[0].transform !== undefined)?.[0];
}

afterEach(() => {
  delete (Element.prototype as { animate?: unknown }).animate;
});

describe("useSheetExit", () => {
  it("closes synchronously when Element.animate is unavailable", () => {
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);

    fireEvent.click(screen.getByText("close"));

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("calls onClose once only after the exit animation finishes", async () => {
    const { animate, finish } = stubAnimate();
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);

    fireEvent.click(screen.getByText("close"));

    expect(animate).toHaveBeenCalledTimes(2);
    expect(onClose).not.toHaveBeenCalled();

    await act(async () => {
      finish();
    });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("ignores a second close requested during the exit", async () => {
    const { animate, finish } = stubAnimate();
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);

    fireEvent.click(screen.getByText("close"));
    fireEvent.click(screen.getByText("close"));

    expect(animate).toHaveBeenCalledTimes(2);

    await act(async () => {
      finish();
    });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes again after a reopen when Element.animate is unavailable", () => {
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);

    fireEvent.click(screen.getByText("close"));
    fireEvent.click(screen.getByText("close"));

    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("closes again after a reopen once the previous exit has finished", async () => {
    const { animate, finish } = stubAnimate();
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);

    fireEvent.click(screen.getByText("close"));
    await act(async () => {
      finish();
    });
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByText("close"));

    expect(animate).toHaveBeenCalledTimes(4);
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(2));
  });

  it("exits a resting card with the sheet-rise mirror, not a full drop (#1611)", async () => {
    const { animate, finish } = stubAnimate();
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />);

    fireEvent.click(screen.getByText("close"));

    expect(cardKeyframes(animate)).toEqual([{ opacity: 0, transform: "translateY(16px)" }]);
    expect(animate).toHaveBeenCalledWith(
      [{ opacity: 0 }],
      expect.objectContaining({ duration: 180, fill: "forwards" }),
    );

    await act(async () => {
      finish();
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps the full slide-down when the card is already dragged down (#1611)", async () => {
    const { animate, finish } = stubAnimate();
    const onClose = vi.fn();
    render(<Harness onClose={onClose} cardStyle={{ transform: "translateY(120px)" }} />);

    fireEvent.click(screen.getByText("close"));

    expect(cardKeyframes(animate)).toEqual([{ transform: "translateY(100%)" }]);

    await act(async () => {
      finish();
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("animates a card-only sheet and calls onClose after the exit finishes with no scrim (#1610)", async () => {
    const { animate, finish } = stubAnimate();
    const onClose = vi.fn();
    function CardOnlyHarness() {
      const { cardRef, requestClose } = useSheetExit(onClose);
      return (
        <div ref={cardRef} data-testid="card">
          <button type="button" onClick={requestClose}>
            close
          </button>
        </div>
      );
    }
    render(<CardOnlyHarness />);

    fireEvent.click(screen.getByText("close"));

    expect(animate).toHaveBeenCalledTimes(1);
    expect(cardKeyframes(animate)).toEqual([{ opacity: 0, transform: "translateY(16px)" }]);
    expect(onClose).not.toHaveBeenCalled();

    await act(async () => {
      finish();
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
