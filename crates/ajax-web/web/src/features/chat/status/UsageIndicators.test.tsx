import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";

describe("ContextUsageMeter", () => {
  it("warns when context pressure is at or above 90%", async () => {
    const { ContextUsageMeter } = await import("./UsageIndicators");
    render(<ContextUsageMeter usage={{ used: 92, size: 100 }} />);
    expect(screen.getByTestId("session-usage")).toHaveClass("is-tight");
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 92% full");
  });

  it("does not apply tight tone below 90%", async () => {
    const { ContextUsageMeter } = await import("./UsageIndicators");
    render(<ContextUsageMeter usage={{ used: 25, size: 100 }} />);
    expect(screen.getByTestId("session-usage")).not.toHaveClass("is-tight");
  });
});
