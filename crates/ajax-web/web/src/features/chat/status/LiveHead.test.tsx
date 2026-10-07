import { describe, it, expect, vi } from "vitest";
import type { ReactNode } from "react";
import { render, screen } from "@testing-library/react";
import LiveHead from "./LiveHead";
import { buildHeadView, headState, headTone, isTaskLevelAttention } from "./headView";
import { initialHeadViewForTests } from "./headView.testHelpers";
import { parseServerFrame } from "../session/transport/parse";
import { projectWireEvent } from "../session/projectWireInput";
import { initialChatSessionReducerState, reduceChatSession } from "../session/reducer";

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
  it("parses usage_reset without payload fields", () => {
    expect(parseServerFrame(JSON.stringify({
      type: "event", protocolVersion: 2, cursor: 7,
      payload: { type: "usage_reset" },
    }))).toEqual({ kind: "event", cursor: 7, event: { type: "usage_reset" } });
  });

  it("clears context and turn usage through the wire projection until new usage arrives", () => {
    let state = reduceChatSession(initialChatSessionReducerState, { type: "prompt", text: "hello" });
    const applyWire = (payload: object) => {
      const frame = parseServerFrame(JSON.stringify({
        type: "event", protocolVersion: 2, cursor: 7, payload,
      }));
      if (!frame || frame.kind !== "event") throw new Error("expected event frame");
      const event = projectWireEvent(frame.event);
      if (!event) throw new Error("expected projected event");
      state = reduceChatSession(state, { type: "event", event });
    };
    const headView = () => buildHeadView({
      session: state.view, taskAttention: null, hasActivity: false,
      activityAgeMs: 0, connected: true,
    });

    applyWire({ type: "usage", used: 90, size: 100 });
    applyWire({ type: "turn_usage", inputTokens: 7 });
    expect(state.view.usage).toEqual({ context: { used: 90, size: 100 }, turn: { inputTokens: 7 } });
    const { rerender } = render(<LiveHead view={headView()} permission={null} onStop={noop} />);
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 90% full");
    const beforeReset = state;

    applyWire({ type: "usage_reset" });
    expect(state.view.usage).toEqual({ context: null, turn: null });
    expect(state.view.conversation).toBe(beforeReset.view.conversation);
    expect(state.view.turn).toBe(beforeReset.view.turn);
    rerender(<LiveHead view={headView()} permission={null} onStop={noop} />);
    expect(screen.queryByTestId("session-usage")).not.toBeInTheDocument();

    applyWire({ type: "usage", used: 5, size: 100 });
    applyWire({ type: "turn_usage", inputTokens: 2 });
    expect(state.view.usage).toEqual({ context: { used: 5, size: 100 }, turn: { inputTokens: 2 } });
    rerender(<LiveHead view={headView()} permission={null} onStop={noop} />);
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 5% full");
  });

  it.each([true, false])("places usage once beside the label when connected=%s", (connected) => {
    mountHead({ connected, showHeadLine: true, usage: { used: 25, size: 100 } });
    const meter = screen.getByTestId("session-usage");
    const label = screen.getByText(connected ? "Ready" : "Reconnecting");
    expect(label.parentElement).toHaveClass("session-head-line");
    expect(label.parentElement).toContainElement(meter);
    expect(label.nextElementSibling).toBe(meter);
    expect(meter.nextElementSibling).toHaveClass("session-head-controls");
    expect(screen.getAllByTestId("session-usage")).toHaveLength(1);
  });

  it("keeps usage once at the bottom when task attention hides the head line", () => {
    mountHead({
      state: "attention",
      showHeadLine: false,
      taskAttention: { status: "waiting" },
      attentionText: "Waiting for review",
      usage: { used: 25, size: 100 },
    });
    expect(screen.getByTestId("session-head").lastElementChild).toBe(
      screen.getByTestId("session-usage"),
    );
    expect(screen.getAllByTestId("session-usage")).toHaveLength(1);
  });

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
