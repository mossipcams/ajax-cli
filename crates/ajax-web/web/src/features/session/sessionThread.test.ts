import { describe, it, expect } from "vitest";
import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
import {
  activePlanStep,
  activeTool,
  explainAcpError,
  explainOpenFailure,
  initialSessionState,
  sessionReducer,
  summarizeTurn,
  toolCallCount,
  type SessionState,
  type ToolCall,
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

describe("ready settles the turn state", () => {
  it("clears a busy thread that replayed history left set", () => {
    const busy = sessionReducer(initialSessionState, {
      type: "event",
      event: { type: "message", role: "agent", text: "working" },
    });
    expect(busy.busy).toBe(true);

    const settled = sessionReducer(busy, {
      type: "event",
      event: { type: "ready", model: "gpt-5.6-sol", busy: false },
    });

    expect(settled.busy).toBe(false);
  });

  it("leaves state alone when the host says nothing about it", () => {
    const busy = sessionReducer(initialSessionState, {
      type: "event",
      event: { type: "message", role: "agent", text: "working" },
    });

    expect(sessionReducer(busy, { type: "event", event: { type: "ready" } }).busy).toBe(true);
  });
});

describe("host notes", () => {
  it("does not put the thread to work", () => {
    // Regression: the restart note arrived as role "agent", which left the head
    // stuck on Working/Thinking with no turn in flight.
    const state = sessionReducer(initialSessionState, {
      type: "event",
      event: {
        type: "message",
        role: "note",
        text: "Model context reset after restart. Prior turns are still visible here.",
      },
    });

    expect(state.busy).toBe(false);
    expect(state.entries.at(-1)).toEqual(
      expect.objectContaining({ kind: "note", tone: "info" }),
    );
  });
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

  it("replaces a cumulative snapshot instead of concatenating it", () => {
    const state = run([
      { type: "message", role: "agent", text: "Hel" },
      { type: "message", role: "agent", text: "Hello" },
    ]);
    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({ kind: "prose", role: "agent", text: "Hello" });
  });

  it("skips an exact duplicate user echo", () => {
    const state = run([
      { type: "message", role: "user", text: "Fix it" },
      { type: "message", role: "user", text: "Fix it" },
    ]);
    expect(state.entries).toHaveLength(1);
    expect(state.entries[0]).toMatchObject({ kind: "prose", role: "user", text: "Fix it" });
  });

  it("starts a new paragraph when a tool run interrupts agent prose", () => {
    const state = run([
      { type: "message", role: "agent", text: "Reading" },
      toolCall("c1"),
      { type: "message", role: "agent", text: "Done" },
    ]);
    expect(state.entries.map((entry) => entry.kind)).toEqual(["prose", "prose"]);
    expect(state.entries[0]).toMatchObject({ text: "Reading" });
    expect(state.entries[1]).toMatchObject({ text: "Done" });
    expect(toolCallCount(state)).toBe(1);
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
    expect(state.entries).toHaveLength(0);
  });

  it("revises a tool call after prose has already arrived", () => {
    const state = run([
      toolCall("c1"),
      { type: "message", role: "agent", text: "thinking out loud" },
      toolCall("c1", { status: "completed" }),
    ]);
    expect(toolCallCount(state)).toBe(1);
    expect(state.entries.map((entry) => entry.kind)).toEqual(["prose"]);
  });

  it("prefers a still-running tool over a finished one in the head", () => {
    const state = run([toolCall("c1", { status: "completed" }), toolCall("c2", { status: "in_progress" })]);
    expect(activeTool(state)?.callId).toBe("c2");
  });

  it("keeps reasoning out of the transcript and the head", () => {
    const thinking = run([
      { type: "message", role: "thought", text: "Checking " },
      { type: "message", role: "thought", text: "the router" },
    ]);
    expect(thinking.busy).toBe(true);
    expect(thinking.entries).toHaveLength(0);

    const answered = sessionReducer(thinking, {
      type: "event",
      event: { type: "message", role: "agent", text: "Found it" },
    });
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

  it("revises a plan in place and never puts it in the transcript", () => {
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
    expect(state.entries).toHaveLength(0);
    expect(state.plan).toEqual([
      { content: "Read", status: "completed" },
      { content: "Patch", status: "in_progress" },
    ]);
    expect(activePlanStep(state.plan)).toBe("Patch");
  });

  it("puts a permission request in the decision slot only, never the transcript", () => {
    const state = run([
      { type: "permission_request", requestId: "7", title: "Run tests?", detail: "cargo test" },
    ]);
    expect(state.decision).toEqual({ requestId: "7", title: "Run tests?", detail: "cargo test" });
    expect(state.entries).toHaveLength(0);
    expect(sessionReducer(state, { type: "decided" }).decision).toBeNull();
  });

  it("clears a resurrected decision when permission_resolved replays", () => {
    const state = run([
      { type: "permission_request", requestId: "7", title: "Run tests?" },
      { type: "permission_resolved", requestId: "7", approved: true },
    ]);
    expect(state.decision).toBeNull();
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
  });

  it("folds a turn's tools into one summary note on settle", () => {
    const state = run([
      toolCall("c1", { kind: "read", status: "completed" }),
      toolCall("c2", { kind: "read", status: "completed" }),
      toolCall("c3", { kind: "edit", status: "completed" }),
      toolCall("c4", { kind: "search", status: "failed" }),
      { type: "turn_end" },
    ]);
    expect(state.busy).toBe(false);
    expect(state.tools).toHaveLength(0);
    expect(state.entries).toEqual([
      expect.objectContaining({ kind: "note", tone: "info", text: "2 read · 1 edit · 1 search · 1 failed" }),
    ]);
  });

  it("does not invent a summary when the turn used no tools", () => {
    const state = run([{ prompt: "hi" }, { type: "message", role: "agent", text: "ok" }, { type: "turn_end" }]);
    expect(state.entries.map((entry) => entry.kind)).toEqual(["prose"]);
  });

  it("ends the turn on error and records it as a transcript note", () => {
    const state = run([{ prompt: "go" }, { type: "error", message: "ACP process exited" }]);
    expect(state.busy).toBe(false);
    expect(state.entries[0]).toMatchObject({
      kind: "note",
      tone: "error",
      text: "The agent stopped. It will restart when you reconnect.",
    });
  });

  it("drops unknown artifacts instead of pretty-printing them", () => {
    const empty = run([{ type: "artifact", kind: "x", title: "", body: "" }]);
    expect(empty.entries).toHaveLength(0);
    const dumped = run([{ type: "artifact", kind: "x", title: "Modes", body: "{}" }]);
    expect(dumped.entries).toHaveLength(0);
  });
});

describe("explainAcpError", () => {
  it("maps opaque ACP failures to operator-facing copy", () => {
    expect(explainAcpError("internal error")).toMatch(/rejected that request/i);
    expect(explainAcpError("ACP process exited")).toMatch(/restart when you reconnect/i);
    expect(explainAcpError("acp request timed out")).toMatch(/did not answer in time/i);
  });

  it("passes through already-human messages", () => {
    const message = "Lost the session connection. Reopen the task to try again.";
    expect(explainAcpError(message)).toBe(message);
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

describe("summarizeTurn", () => {
  const call = (kind: string, status: ToolCall["status"] = "completed"): ToolCall => ({
    callId: kind,
    title: kind,
    kind,
    status,
    locations: [],
  });

  it("returns null for an empty turn", () => {
    expect(summarizeTurn([])).toBeNull();
  });

  it("keeps first-seen kind order", () => {
    expect(summarizeTurn([call("edit"), call("read"), call("edit")])).toBe("2 edit · 1 read");
  });
});
