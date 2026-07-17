import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/react";
import ResultPanel from "./ResultPanel";
import { DROP_UNDO_MS } from "../polling";

describe("ResultPanel", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("renders the message and output", () => {
    const { getByText, container } = render(
      <ResultPanel message="Review completed" output="logs here" isError={false} />,
    );
    expect(getByText("Review completed")).toBeInTheDocument();
    expect(container.querySelector(".result-output")?.textContent).toContain("logs here");
  });

  it("applies the error styling for failures", () => {
    const { container } = render(<ResultPanel message="Action failed" isError={true} />);
    expect(container.querySelector(".result-panel")?.classList.contains("is-error")).toBe(true);
  });

  it("calls onDismiss when dismissed", async () => {
    const onDismiss = vi.fn();
    const { getByText } = render(
      <ResultPanel message="Done" isError={false} onDismiss={onDismiss} />,
    );
    await fireEvent.click(getByText("Dismiss"));
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it("auto-dismisses success toasts after 4s", () => {
    const onDismiss = vi.fn();
    render(<ResultPanel message="Done" isError={false} onDismiss={onDismiss} />);
    expect(onDismiss).not.toHaveBeenCalled();
    vi.advanceTimersByTime(4000);
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it("keeps error toasts up longer than success toasts", () => {
    const onDismiss = vi.fn();
    render(<ResultPanel message="Boom" isError={true} onDismiss={onDismiss} />);
    vi.advanceTimersByTime(4000);
    expect(onDismiss).not.toHaveBeenCalled();
    vi.advanceTimersByTime(8000);
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it("announces errors assertively", () => {
    const { container, rerender } = render(<ResultPanel message="x" isError={true} />);
    const panel = container.querySelector(".result-panel");
    expect(panel).toHaveAttribute("role", "alert");
    expect(panel).toHaveAttribute("aria-live", "assertive");

    rerender(<ResultPanel message="ok" isError={false} />);
    expect(panel).toHaveAttribute("role", "status");
    expect(panel).toHaveAttribute("aria-live", "polite");
  });

  it("shows an Undo button when onUndo is set and calls it on click", async () => {
    const onUndo = vi.fn();
    const { getByText } = render(<ResultPanel message="Dropping web/x…" onUndo={onUndo} />);
    expect(getByText("Undo")).toBeInTheDocument();
    await fireEvent.click(getByText("Undo"));
    expect(onUndo).toHaveBeenCalledOnce();
  });

  it("auto-dismisses and calls onCommit after the undo window when armed", () => {
    const onCommit = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ResultPanel message="Dropping web/x…" onCommit={onCommit} onDismiss={onDismiss} />,
    );
    expect(onCommit).not.toHaveBeenCalled();
    vi.advanceTimersByTime(DROP_UNDO_MS);
    expect(onCommit).toHaveBeenCalledOnce();
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
