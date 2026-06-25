import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ResultPanel from "./ResultPanel.svelte";

describe("ResultPanel", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("renders the message and output", () => {
    const { getByText, container } = render(ResultPanel, {
      props: { message: "Review completed", output: "logs here", isError: false },
    });
    expect(getByText("Review completed")).toBeInTheDocument();
    expect(container.querySelector(".result-output")?.textContent).toContain("logs here");
  });

  it("applies the error styling for failures", () => {
    const { container } = render(ResultPanel, {
      props: { message: "Action failed", isError: true },
    });
    expect(container.querySelector(".result-panel")?.classList.contains("is-error")).toBe(true);
  });

  it("calls onDismiss when dismissed", async () => {
    const onDismiss = vi.fn();
    const { getByText } = render(ResultPanel, {
      props: { message: "Done", isError: false, onDismiss },
    });
    await fireEvent.click(getByText("Dismiss"));
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it("auto-dismisses after the timeout", () => {
    const onDismiss = vi.fn();
    render(ResultPanel, { props: { message: "Done", isError: false, onDismiss } });
    expect(onDismiss).not.toHaveBeenCalled();
    vi.advanceTimersByTime(12000);
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
