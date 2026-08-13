import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import TaskLoadError from "./TaskLoadError";

describe("TaskLoadError", () => {
  it("fires onRetry only once under same-turn double click", () => {
    const onRetry = vi.fn();
    render(<TaskLoadError message="boom" onRetry={onRetry} />);
    const retry = screen.getByRole("button", { name: "Retry" });
    retry.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    retry.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});
