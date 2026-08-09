import { describe, it, expect } from "vitest";
import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
import {
  activeTool,
  explainOpenFailure,
  initialSessionState,
  sessionReducer,
  toolCallCount,
  thoughtTail,
  type SessionState,
} from "./sessionThread";

function run(events: (WebSessionServerEvent | { prompt: string })[]): SessionState {
  return events.reduce<SessionState>(
    (state, event) =>
      "prompt" in event
        ? sessionReducer(state, { type: "prompt", text: event.prompt })
        : sessionReducer(state, { type: "event", event }),
    initialSessionState,
  );
}

const toolCall = (
  callId: string,
  overrides: Partial<Extract<WebSessionServerEvent, { type: "tool_call" }>> = {},
): WebSessionServerEvent => ({
  type: "tool_call",
  callId,
  title: "Read config",
  kind: "read",
  status: "pending",
  locations: ["/repo/src/config.ts"],
  ...overrides,
});

describe("sessionReducer", () => {
  it("coalesces consecutive agent chunks into one paragraph", () => {
    const state = run([
      { type: "message", role: "agent", text: "Hello " },
      { type: "message", role: "agent", text: "world" },
    ]);
    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({ kind: "prose", role: "agent", text: "Hello world" });
  });

  it("starts a new paragraph when a tool run interrupts agent prose", () => {
    const state = run([
      { type: "message", role: "agent", text: "Reading" },
      toolCall("c1"),
      { type: "message", role: "agent", text: "Done" },
    ]);
    expect(state.entries.map((entry) => entry.kind)).toEqual(["prose", "tools", "prose"]);
  });

  it("merges tool_call_update into the open call instead of appending a row", () => {
    const state = run([
      toolCall("c1"),
      toolCall("c1", { status: "completed", title: "", kind: "", locations: [] }),
    ]);
    expect(toolCallCount(state)).toBe(1);
    expect(activeTool(state)).toMatchObject({
      callId: "c1",
      status: "completed",
      title: "Read config",
      kind: "read",
      locations: ["/repo/src/config.ts"],
    });
  });

  it("revises a tool call that prose has already scrolled past", () => {
    const state = run([
      toolCall("c1"),
      { type: "message", role: "agent", text: "thinking out loud" },
      toolCall("c1", { status: "completed" }),
    ]);
    expect(toolCallCount(state)).toBe(1);
    expect(state.entries.map((entry) => entry.kind)).toEqual(["tools", "prose"]);
  });

  it("prefers a still-running tool over a finished one in the head", () => {
    const state = run([toolCall("c1", { status: "completed" }), toolCall("c2", { status: "in_progress" })]);
    expect(activeTool(state)?.callId).toBe("c2");
  });

  it("keeps reasoning out of the transcript and clears it on real output", () => {
    const thinking = run([
      { type: "message", role: "thought", text: "Checking " },
      { type: "message", role: "thought", text: "the router" },
    ]);
    expect(thinking.thought).toBe("Checking the router");
    expect(thinking.entries).toHaveLength(0);

    const answered = sessionReducer(thinking, {
      type: "event",
      event: { type: "message", role: "agent", text: "Found it" },
    });
    expect(answered.thought).toBeNull();
    expect(answered.entries).toHaveLength(1);
  });

  it("replaces run status in the head rather than appending it to the transcript", () => {
    const state = run([
      { type: "status", state: "running" },
      { type: "status", state: "waiting" },
    ]);
    expect(state.entries).toHaveLength(0);
    expect(state.status).toBe("waiting");
  });

  it("revises a plan in place so a long turn does not stack plan cards", () => {
    const state = run([
      { type: "plan", entries: [{ content: "Read", status: "pending" }] },
      {
        type: "plan",
        entries: [
          { content: "Read", status: "completed" },
          { content: "Patch", status: "in_progress" },
        ],
      },
    ]);
    const plans = state.entries.filter((entry) => entry.kind === "plan");
    expect(plans).toHaveLength(1);
    expect(plans[0]).toMatchObject({ entries: [{ status: "completed" }, { status: "in_progress" }] });
  });

  it("puts a permission request in the decision slot only, never the transcript", () => {
    const state = run([
      { type: "permission_request", requestId: "7", title: "Run tests?", detail: "cargo test" },
    ]);
    expect(state.decision).toEqual({ requestId: "7", title: "Run tests?", detail: "cargo test" });
    expect(state.entries).toHaveLength(0);
    expect(sessionReducer(state, { type: "decided" }).decision).toBeNull();
  });

  it("tracks the turn: busy on prompt, settled on turn_end", () => {
    const busy = run([{ prompt: "Fix the test" }]);
    expect(busy.busy).toBe(true);
    // The host owns the transcript and streams the prompt back, so sending
    // marks the turn in flight without writing an entry the log lacks.
    expect(busy.entries).toHaveLength(0);
    const echoed = sessionReducer(busy, {
      type: "event",
      event: { type: "message", role: "user", text: "Fix the test" },
    });
    expect(echoed.entries[0]).toMatchObject({ kind: "prose", role: "user", text: "Fix the test" });

    const settled = sessionReducer(busy, { type: "event", event: { type: "turn_end" } });
    expect(settled.busy).toBe(false);
    expect(settled.thought).toBeNull();
  });

  it("ends the turn on error and records it as a transcript note", () => {
    const state = run([{ prompt: "go" }, { type: "error", message: "ACP process exited" }]);
    expect(state.busy).toBe(false);
    expect(state.entries[0]).toMatchObject({ kind: "note", tone: "error", text: "ACP process exited" });
  });

  it("drops empty artifacts and keeps ones carrying a body", () => {
    const empty = run([{ type: "artifact", kind: "x", title: "", body: "" }]);
    expect(empty.entries).toHaveLength(0);
    const kept = run([{ type: "artifact", kind: "x", title: "Modes", body: "{}" }]);
    expect(kept.entries[0]).toMatchObject({ kind: "note", text: "Modes", body: "{}" });
  });
});

describe("explainOpenFailure", () => {
  it("names the agent when the task cannot host an orchestration session", () => {
    const message = explainOpenFailure({ agent: "Claude", status_explanation: "Running" });
    expect(message).toContain("Cursor");
    expect(message).toContain("Claude");
  });

  it("passes through the server's own explanation for a Cursor task", () => {
    expect(
      explainOpenFailure({ agent: "Cursor", status_explanation: "Worktree missing" }),
    ).toContain("Worktree missing");
  });

  it("still says something actionable with no detail at all", () => {
    expect(explainOpenFailure(null)).toMatch(/worktree/i);
  });
});

describe("thoughtTail", () => {
  it("keeps short reasoning verbatim", () => {
    expect(thoughtTail("Checking the router")).toBe("Checking the router");
  });

  it("keeps the tail, not the opening, once reasoning runs long", () => {
    const long = "word ".repeat(200) + "the actual latest thought";
    const tail = thoughtTail(long);
    expect(tail).toContain("the actual latest thought");
    expect(tail.startsWith("…")).toBe(true);
    expect(tail.length).toBeLessThan(200);
  });

  it("collapses newlines so the head stays two readable lines", () => {
    expect(thoughtTail("one\n\n  two")).toBe("one two");
  });

  it("bounds growth across a whole streaming turn", () => {
    let state = initialSessionState;
    for (let i = 0; i < 400; i += 1) {
      state = sessionReducer(state, {
        type: "event",
        event: { type: "message", role: "thought", text: `chunk ${i} ` },
      });
    }
    expect((state.thought ?? "").length).toBeLessThan(200);
    expect(state.thought).toContain("chunk 399");
  });
});
