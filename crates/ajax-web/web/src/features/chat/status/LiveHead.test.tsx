import { describe, it, expect, vi } from "vitest";
import type { ReactNode } from "react";
import { render, screen } from "@testing-library/react";
import LiveHead from "./LiveHead";
import { buildHeadView, headState, headTone, isTaskLevelAttention } from "./headView";
import { initialHeadViewForTests } from "./headView.testHelpers";

const noop = vi.fn();

function mountHead(
  viewOverrides: Parameters<typeof initialHeadViewForTests>[0] = {},
  extra?: { actions?: ReactNode; permission?: ReactNode },
) {
  const view = initialHeadViewForTests(viewOverrides);
  return render(
    <LiveHead
      view={view}
      permission={extra?.permission ?? null}
      actions={extra?.actions}
      onStop={noop}
    />,
  );
}

describe("headState precedence", () => {
  it("prefers permission decision over agent status", () => {
    expect(
      headState({ requestId: "1", title: "Run?", detail: "" }, null, false, null, "running"),
    ).toBe("decision");
  });

  it("maps ACP waiting and requires_action to attention", () => {
    expect(headState(null, null, false, null, "waiting")).toBe("attention");
    expect(headState(null, null, false, null, "requires_action")).toBe("attention");
  });

  it("maps ACP running or session busy to working", () => {
    expect(headState(null, null, false, null, "running")).toBe("working");
    expect(headState(null, null, true, null, "idle")).toBe("working");
  });

  it("maps task attention waiting/error to attention", () => {
    expect(headState(null, null, false, { status: "waiting" }, "idle")).toBe("attention");
    expect(headState(null, null, false, { status: "error" }, "idle")).toBe("attention");
  });

  it("defaults to idle when nothing else applies", () => {
    expect(headState(null, null, false, null, "idle")).toBe("idle");
    expect(headState(null, null, false, null, null)).toBe("idle");
  });
});

describe("headTone", () => {
  it("uses error tone for task attention errors", () => {
    expect(headTone("attention", { status: "error" })).toBe("error");
  });
});

describe("isTaskLevelAttention", () => {
  it("is true for task waiting or error without an ACP decision", () => {
    expect(isTaskLevelAttention("attention", { status: "waiting" }, null)).toBe(true);
    expect(isTaskLevelAttention("attention", { status: "error" }, null)).toBe(true);
  });

  it("is false when an ACP decision or non-attention state owns the head", () => {
    expect(
      isTaskLevelAttention(
        "attention",
        { status: "waiting" },
        { requestId: "1", title: "Run?", detail: "" },
      ),
    ).toBe(false);
    expect(isTaskLevelAttention("working", { status: "waiting" }, null)).toBe(false);
  });
});

describe("LiveHead task attention chrome", () => {
  it("shows one explanation line without duplicating the needs-you label", () => {
    mountHead(
      {
        state: "attention",
        tone: "waiting",
        taskAttention: { status: "waiting", explanation: "Waiting for review" },
        attentionText: "Waiting for review",
        showHeadLine: false,
      },
      { actions: <button type="button">Review</button> },
    );
    expect(screen.queryByText("Needs you")).not.toBeInTheDocument();
    expect(screen.getByTestId("session-attention")).toHaveTextContent("Waiting for review");
    expect(screen.getByRole("button", { name: "Review" })).toBeInTheDocument();
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

describe("LiveHead working quiet lines", () => {
  it("shows tool and plan step together without the quiet fallback", () => {
    mountHead({
      state: "working",
      tone: "running",
      tool: {
        callId: "c1",
        kind: "edit",
        tone: "running",
        mark: "±",
        title: "Edit config",
        path: "…/src/config.ts",
        statusLabel: "running",
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

describe("buildHeadView", () => {
  it("derives task-level attention without BrowserTaskDetail", () => {
    const view = buildHeadView({
      session: {
        conversation: [],
        turn: { busy: false, proseOpen: true },
        permission: { decision: null, resolvedIds: [] },
        elicitation: { decision: null, resolvedIds: [] },
        status: { acpState: "idle", detail: null },
        usage: { context: null, turn: null },
        model: {},
        revision: 0,
      },
      taskAttention: { status: "waiting", explanation: "Needs input" },
      tool: null,
      planStep: null,
      thoughtSnippet: null,
      activityAgeMs: 0,
      connected: true,
    });
    expect(view.state).toBe("attention");
    expect(view.showHeadLine).toBe(false);
    expect(view.attentionText).toBe("Needs input");
  });
});
