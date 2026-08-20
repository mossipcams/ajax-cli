import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import LiveHead, { formatTurnUsage, headState, headTone } from "./LiveHead";

const noop = vi.fn();

function mountHead(
  overrides: Partial<React.ComponentProps<typeof LiveHead>> = {},
) {
  return render(
    <LiveHead
      state="idle"
      tone="idle"
      detail={null}
      decision={null}
      tool={null}
      planStep={null}
      thoughtSnippet={null}
      usage={null}
      turnUsage={null}
      activityAgeMs={0}
      connected
      onApprove={noop}
      onReject={noop}
      onStop={noop}
      {...overrides}
    />,
  );
}

describe("headState precedence", () => {
  it("prefers permission decision over agent status", () => {
    expect(
      headState({ requestId: "1", title: "Run?", detail: "" }, true, null, "running"),
    ).toBe("decision");
  });

  it("maps ACP waiting and requires_action to attention", () => {
    expect(headState(null, false, null, "waiting")).toBe("attention");
    expect(headState(null, false, null, "requires_action")).toBe("attention");
  });

  it("maps ACP running or session busy to working", () => {
    expect(headState(null, false, null, "running")).toBe("working");
    expect(headState(null, true, null, "idle")).toBe("working");
  });

  it("maps task detail waiting/error to attention", () => {
    expect(headState(null, false, { status: "waiting" } as never, "idle")).toBe("attention");
    expect(headState(null, false, { status: "error" } as never, "idle")).toBe("attention");
  });

  it("defaults to idle when nothing else applies", () => {
    expect(headState(null, false, null, "idle")).toBe("idle");
    expect(headState(null, false, null, null)).toBe("idle");
  });
});

describe("headTone", () => {
  it("uses error tone for task detail errors", () => {
    expect(headTone("attention", { status: "error" } as never)).toBe("error");
  });
});

describe("LiveHead context usage", () => {
  it("shows reported usage in idle below 70%", () => {
    mountHead({ state: "idle", usage: { used: 25, size: 100 } });
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 25% full");
  });

  it("shows reported usage while working below 70%", () => {
    mountHead({
      state: "working",
      tone: "running",
      usage: { used: 40, size: 100 },
      thoughtSnippet: "Checking files",
    });
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 40% full");
  });

  it("does not render a meter when usage is absent", () => {
    mountHead({ state: "idle", usage: null });
    expect(screen.queryByTestId("session-usage")).not.toBeInTheDocument();
  });

  it("warns when context pressure is at or above 90%", () => {
    mountHead({ state: "idle", usage: { used: 92, size: 100 } });
    expect(screen.getByTestId("session-usage")).toHaveClass("is-tight");
  });
});

describe("LiveHead turn usage", () => {
  it("does not render turn token counts in the head", () => {
    mountHead({
      state: "idle",
      turnUsage: { inputTokens: 1200, outputTokens: 450, totalTokens: 1650 },
    });
    expect(screen.queryByTestId("session-turn-usage")).not.toBeInTheDocument();
  });

  it("still shows context usage when turn usage is present", () => {
    mountHead({
      state: "idle",
      usage: { used: 30, size: 100 },
      turnUsage: { inputTokens: 500, totalTokens: 500 },
    });
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 30% full");
    expect(screen.queryByTestId("session-turn-usage")).not.toBeInTheDocument();
  });
});

describe("formatTurnUsage", () => {
  it("returns null when no token fields are present", () => {
    expect(formatTurnUsage({})).toBeNull();
    expect(formatTurnUsage({ requestId: "x" })).toBeNull();
  });
});

describe("LiveHead working quiet lines", () => {
  it("shows tool and plan step together without the quiet fallback", () => {
    mountHead({
      state: "working",
      tone: "running",
      tool: {
        callId: "c1",
        title: "Edit config",
        kind: "edit",
        status: "in_progress",
        locations: ["/repo/src/config.ts"],
        content: [],
      },
      planStep: "Patch the port",
      thoughtSnippet: "Checking files",
    });
    expect(screen.getByTestId("session-head-tool")).toHaveTextContent("Edit config");
    expect(screen.getByTestId("session-plan-step")).toHaveTextContent("Patch the port");
    expect(screen.queryByTestId("session-head-thought")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-head-idle")).not.toBeInTheDocument();
  });

  it("shows thought in the quiet fallback when there is no tool or plan step", () => {
    mountHead({
      state: "working",
      tone: "running",
      thoughtSnippet: "Checking files",
    });
    expect(screen.getByTestId("session-head-thought")).toHaveTextContent("Checking files");
    expect(screen.queryByTestId("session-head-idle")).not.toBeInTheDocument();
  });

  it("falls back to Thinking when nothing else is live yet", () => {
    mountHead({ state: "working", tone: "running" });
    expect(screen.getByTestId("session-head-idle")).toHaveTextContent("Thinking…");
  });

  it("shows Working as the primary label when state is working", () => {
    mountHead({ state: "working", tone: "running" });
    expect(screen.getByText("Working")).toBeInTheDocument();
  });
});
