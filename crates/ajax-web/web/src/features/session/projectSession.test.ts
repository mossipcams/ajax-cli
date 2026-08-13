import { describe, expect, it } from "vitest";
import { activePlanStep, activeTool, projectSession, type ServerEvent } from "./projectSession";

describe("projectSession", () => {
  it("keeps thought messages off the transcript", () => {
    const events: ServerEvent[] = [
      { type: "message", role: "thought", text: "thinking quietly" },
      { type: "message", role: "agent", text: "visible reply" },
    ];
    const view = projectSession(events);
    expect(view.entries).toHaveLength(1);
    expect(view.entries[0]).toMatchObject({ kind: "prose", role: "agent", text: "visible reply" });
    expect(view.busy).toBe(true);
  });

  it("shows tools in the head only, never in the transcript", () => {
    const events: ServerEvent[] = [
      {
        type: "tool_call",
        callId: "c1",
        title: "Read router",
        kind: "read",
        status: "in_progress",
        locations: ["/repo/router.rs"],
      },
      { type: "message", role: "agent", text: "Found the guard." },
    ];
    const view = projectSession(events);
    expect(activeTool(view)?.title).toBe("Read router");
    expect(view.tools).toHaveLength(1);
    expect(view.entries.some((entry) => entry.kind !== "prose")).toBe(false);
    expect(JSON.stringify(view.entries)).not.toContain("sessionUpdate");
  });

  it("surfaces permission requests in the head, not the transcript", () => {
    const events: ServerEvent[] = [
      {
        type: "permission_request",
        requestId: "42",
        title: "Run tests?",
        detail: "cargo nextest",
      },
    ];
    const view = projectSession(events);
    expect(view.decision).toMatchObject({ requestId: "42", title: "Run tests?" });
    expect(view.entries).toHaveLength(0);
  });

  it("adds a turn_end summary note and clears live tools", () => {
    const events: ServerEvent[] = [
      {
        type: "tool_call",
        callId: "c1",
        title: "Search",
        kind: "search",
        status: "completed",
      },
      {
        type: "tool_call",
        callId: "c2",
        title: "Edit",
        kind: "edit",
        status: "completed",
      },
      { type: "turn_end", stopReason: "end_turn" },
    ];
    const view = projectSession(events);
    expect(view.busy).toBe(false);
    expect(view.tools).toHaveLength(0);
    expect(view.entries.at(-1)).toMatchObject({
      kind: "note",
      tone: "info",
      text: "1 search · 1 edit",
    });
  });

  it("clears a decision when permission_resolved matches", () => {
    const events: ServerEvent[] = [
      {
        type: "permission_request",
        requestId: "42",
        title: "Run tests?",
      },
      { type: "permission_resolved", requestId: "42", approved: true },
    ];
    const view = projectSession(events);
    expect(view.decision).toBeNull();
  });

  it("keeps the in-progress plan step on the head", () => {
    const events: ServerEvent[] = [
      {
        type: "plan",
        entries: [
          { content: "First", status: "completed" },
          { content: "Second", status: "in_progress" },
        ],
      },
    ];
    expect(activePlanStep(projectSession(events).plan)).toBe("Second");
  });

  it("starts a new agent bubble for a finished utterance, not a stream chunk", () => {
    const events: ServerEvent[] = [
      { type: "message", role: "agent", text: "First sentence is done." },
      { type: "message", role: "agent", text: "Second sentence stands alone." },
      { type: "message", role: "agent", text: "Second sentence stands alone. And grows." },
      { type: "message", role: "agent", text: " token" },
    ];
    const view = projectSession(events);
    expect(view.entries).toHaveLength(2);
    expect(view.entries[0]).toMatchObject({ text: "First sentence is done." });
    expect(view.entries[1]).toMatchObject({
      text: "Second sentence stands alone. And grows. token",
    });
  });
});
