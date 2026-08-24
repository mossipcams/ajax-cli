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

describe("LiveHead connection badge", () => {
  // #1039: the state label and the offline badge rendered together, so the
  // head claimed Ready and Reconnecting at the same time.
  it("shows one badge: a dropped socket replaces the state label", () => {
    mountHead({ state: "idle", connected: false });

    expect(screen.getByTestId("session-head-offline")).toHaveTextContent("Reconnecting");
    expect(screen.queryByText("Ready")).not.toBeInTheDocument();
  });

  it("shows the state label while connected", () => {
    mountHead({ state: "idle", connected: true });

    expect(screen.getByText("Ready")).toBeInTheDocument();
    expect(screen.queryByTestId("session-head-offline")).not.toBeInTheDocument();
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
      hasActivity: true,
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
  // The head used to reprint the running tool and the active plan step that the
  // turn's activity row already narrates in the transcript — the same command
  // twice, a screen apart, with the void between them.
  it("leaves the operation to the transcript once the turn has activity", () => {
    mountHead({ state: "working", tone: "running", hasActivity: true });
    expect(screen.queryByTestId("session-head-tool")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-plan-step")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-head-thought")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-head-idle")).not.toBeInTheDocument();
  });

  it("says Thinking only before the first event, when the transcript is empty", () => {
    mountHead({ state: "working", tone: "running", hasActivity: false });
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
      hasActivity: false,
      activityAgeMs: 0,
      connected: true,
    });
    expect(view.state).toBe("attention");
    expect(view.showHeadLine).toBe(false);
    expect(view.attentionText).toBe("Needs input");
  });

  // Task attention replaces the head line, and `Reconnecting` lives on it. A
  // task waiting for review is where most sessions rest, so a dropped socket
  // used to be invisible in exactly the common case.
  it("keeps the head line under task attention while the socket is down", () => {
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
      hasActivity: false,
      activityAgeMs: 0,
      connected: false,
    });
    expect(view.showHeadLine).toBe(true);

    mountHead({ ...view, state: view.state, connected: false });
    expect(screen.getByTestId("session-head-offline")).toBeInTheDocument();
  });
});
