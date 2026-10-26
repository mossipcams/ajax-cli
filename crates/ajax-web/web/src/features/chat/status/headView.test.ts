import { describe, it, expect } from "vitest";
import type { ChatSessionView } from "../session/public";
import { buildHeadView, headState, headStateLabel, headTone, isTaskLevelAttention } from "./headView";

function session(busy: boolean): ChatSessionView {
  return {
    conversation: [],
    turn: { busy, proseOpen: true },
    permission: { decision: null, resolvedIds: [] },
    elicitation: { decision: null, resolvedIds: [] },
    usage: { context: null },
    model: {},
    revision: 0,
  } as unknown as ChatSessionView;
}

describe("headView", () => {
  describe("headState precedence", () => {
    it("prefers permission decision over agent status", () => {
      expect(
        headState({ requestId: "1", title: "Run?", detail: "" }, null, false, null),
      ).toBe("decision");
    });

    it("prefers elicitation decision over agent status", () => {
      expect(
        headState(null, { requestId: "e1", message: "Pick env", schema: {}, fields: [] }, false, null),
      ).toBe("decision");
    });

    it("prefers a pending decision over a running server task", () => {
      expect(
        headState({ requestId: "1", title: "Run?", detail: "" }, null, false, {
          status: "running",
        }),
      ).toBe("decision");
    });

    it("maps a running server task to working even when the stream is not busy (background work)", () => {
      expect(
        headState(null, null, false, { status: "running", explanation: "Background work running" }),
      ).toBe("working");
    });

    it("maps a running server task to working when the stream is busy", () => {
      expect(headState(null, null, true, { status: "running", explanation: "Agent working" })).toBe(
        "working",
      );
    });

    it("lets a loaded idle server task win over a busy stream", () => {
      expect(headState(null, null, true, { status: "idle", explanation: null })).toBe("idle");
    });

    it("maps unknown and idle server tasks to idle when the stream is not busy", () => {
      expect(headState(null, null, false, { status: "idle" })).toBe("idle");
      expect(headState(null, null, false, { status: "unknown" })).toBe("idle");
    });

    it("maps waiting and error server tasks to attention", () => {
      expect(headState(null, null, false, { status: "waiting" })).toBe("attention");
      expect(headState(null, null, false, { status: "error" })).toBe("attention");
    });

    it("falls back to the stream busy flag when task detail is unavailable", () => {
      expect(headState(null, null, true, null)).toBe("working");
      expect(headState(null, null, false, null)).toBe("idle");
    });
  });

  describe("headStateLabel", () => {
    it("shows Working for working without quiet", () => {
      expect(headStateLabel("working", false)).toBe("Working");
    });

    it("shows No recent activity for quiet working", () => {
      expect(headStateLabel("working", true)).toBe("No recent activity");
    });

    it("shows Ready for idle", () => {
      expect(headStateLabel("idle", false)).toBe("Ready");
    });
  });

  describe("buildHeadView", () => {
    it("carries the running server explanation onto the working head", () => {
      const view = buildHeadView({
        session: session(false),
        taskAttention: { status: "running", explanation: "Background work running" },
        hasActivity: false,
        activityAgeMs: 0,
        connected: true,
      });
      expect(view.state).toBe("working");
      expect(view.tone).toBe("running");
      expect(view.workingText).toBe("Background work running");
      expect(view.taskAttention).toBeNull();
    });

    it("exposes the stream busy flag on the head view", () => {
      const busy = buildHeadView({
        session: session(true),
        taskAttention: null,
        hasActivity: false,
        activityAgeMs: 0,
        connected: true,
      });
      const idle = buildHeadView({
        session: session(false),
        taskAttention: { status: "running", explanation: "Background work running" },
        hasActivity: false,
        activityAgeMs: 0,
        connected: true,
      });
      expect(busy.streamBusy).toBe(true);
      expect(idle.streamBusy).toBe(false);
    });

    it("leaves workingText empty when the running task has no explanation", () => {
      const view = buildHeadView({
        session: session(false),
        taskAttention: { status: "running", explanation: "  " },
        hasActivity: false,
        activityAgeMs: 0,
        connected: true,
      });
      expect(view.state).toBe("working");
      expect(view.workingText).toBeNull();
    });

    it("shows Ready for a loaded idle task even when the stream is busy", () => {
      const view = buildHeadView({
        session: session(true),
        taskAttention: { status: "idle", explanation: null },
        hasActivity: false,
        activityAgeMs: 0,
        connected: true,
      });
      expect(view.state).toBe("idle");
      expect(view.workingText).toBeNull();
    });

    it("uses the stream busy flag when task detail is unavailable", () => {
      const view = buildHeadView({
        session: session(true),
        taskAttention: null,
        hasActivity: false,
        activityAgeMs: 0,
        connected: true,
      });
      expect(view.state).toBe("working");
      expect(view.workingText).toBeNull();
    });
  });

  describe("headTone", () => {
    it("uses error tone for task attention errors", () => {
      expect(headTone("attention", { status: "error" })).toBe("error");
    });
  });

  describe("isTaskLevelAttention", () => {
    it("is false when an ACP decision owns the head", () => {
      expect(
        isTaskLevelAttention(
          "attention",
          { status: "waiting" },
          { requestId: "1", title: "Run?", detail: "" },
        ),
      ).toBe(false);
    });
  });
});
