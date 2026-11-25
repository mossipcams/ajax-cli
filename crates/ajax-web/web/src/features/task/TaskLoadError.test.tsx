import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import TaskLoadError from "./TaskLoadError";

describe("TaskLoadError", () => {
  it("fires onRetry only once under same-turn double click", () => {
    const onRetry = vi.fn();
    render(<TaskLoadError message="boom" onBack={vi.fn()} onRetry={onRetry} />);
    const retry = screen.getByRole("button", { name: "Retry" });
    retry.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    retry.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("renders a Back control beside Retry that calls onBack (#1627)", () => {
    const onBack = vi.fn();
    const onRetry = vi.fn();
    render(<TaskLoadError message="boom" onBack={onBack} onRetry={onRetry} />);
    fireEvent.click(screen.getByRole("button", { name: "← Back" }));
    expect(onBack).toHaveBeenCalledOnce();
    expect(onRetry).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});
