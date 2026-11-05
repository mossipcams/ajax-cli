import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import { useSheetExit } from "./useSheetExit";

function Harness({ onClose }: { onClose: () => void }) {
  const { scrimRef, cardRef, requestClose } = useSheetExit(onClose);
  return (
    <div ref={scrimRef} data-testid="scrim">
      <div ref={cardRef} data-testid="card">
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
});
