import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { formatTurnUsage } from "./UsageIndicators";

describe("formatTurnUsage", () => {
  it("returns null when no token fields are present", () => {
    expect(formatTurnUsage({})).toBeNull();
    expect(formatTurnUsage({ requestId: "x" })).toBeNull();
  });

  it("formats only fields that are present numbers", () => {
    expect(formatTurnUsage({ inputTokens: 1200, outputTokens: 450 })).toBe(
      "Turn tokens: input 1,200 · output 450",
    );
  });

  it("never renders missing counts as zero", () => {
    const formatted = formatTurnUsage({ inputTokens: 0, outputTokens: 100 });
    expect(formatted).toBe("Turn tokens: input 0 · output 100");
    expect(formatTurnUsage({ totalTokens: undefined as never })).toBeNull();
  });
});

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
