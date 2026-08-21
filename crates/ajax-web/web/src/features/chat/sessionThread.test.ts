import { describe, it, expect, vi } from "vitest";
import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
import {
  activePlanStep,
  activeTool,
  explainAcpError,
  explainOpenFailure,
  initialSessionState,
  latestPlan,
  latestThought,
  sessionReducer,
  thoughtSnippet,
  toolCount,
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

const agentMsg = (
  text: string,
  itemId = "i-agent",
  messageId?: string,
): WebSessionServerEvent => ({
  type: "message",
  role: "agent",
  text,
  itemId,
  ...(messageId ? { messageId } : {}),
});
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
      event: agentMsg("working"),
    });
    expect(busy.busy).toBe(true);

    const settled = sessionReducer(busy, {
      type: "event",
      event: { type: "ready", model: "gpt-5.6-sol", busy: false, reset: false },
    });

    expect(settled.busy).toBe(false);
  });

  it("leaves state alone when the host says nothing about it", () => {
    const busy = sessionReducer(initialSessionState, {
      type: "event",
      event: agentMsg("working"),
    });

    expect(sessionReducer(busy, { type: "event", event: { type: "ready" } }).busy).toBe(true);
  });

  it("preserves items on incremental reconnect when reset is false", () => {
    const prior = run([agentMsg("one", "i1"), agentMsg("two", "i2")]);
    const reconnected = sessionReducer(prior, {
      type: "event",
      event: { type: "ready", busy: false, reset: false },
    });
    const afterTail = sessionReducer(reconnected, {
      type: "event",
      event: agentMsg("three", "i3"),
    });
    expect(afterTail.items).toHaveLength(3);
    expect(afterTail.items.map((item) => (item.kind === "prose" ? item.text : null))).toEqual([
      "one",
      "two",
      "three",
    ]);
  });

  it("clears reducer when snapshot reset is true before replay tail", () => {
    const prior = run([agentMsg("one", "i1")]);
    const reset = sessionReducer(prior, {
      type: "event",
      event: { type: "ready", busy: false, reset: true },
    });
    const afterTail = sessionReducer(reset, {
      type: "event",
      event: agentMsg("two", "i2"),
    });
    expect(afterTail.items).toHaveLength(1);
    expect(afterTail.items[0]).toMatchObject({ kind: "prose", role: "agent", text: "two", id: "i2" });
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
    expect(state.items.at(-1)).toEqual(expect.objectContaining({ kind: "note", tone: "info" }));
  });
});

describe("sessionReducer", () => {
  it("renders legacy agent messages without itemId", () => {
    const state = run([{ type: "message", role: "agent", text: "Hello from v1" }]);
    expect(state.items).toHaveLength(1);
    expect(state.items[0]).toMatchObject({ kind: "prose", role: "agent", text: "Hello from v1" });
  });

  it("replaces host item updates by itemId", () => {
    const state = run([
      agentMsg("Hello ", "i1"),
      agentMsg("Hello world", "i1"),
    ]);
    expect(state.items).toHaveLength(1);
    expect(state.items[0]).toMatchObject({ kind: "prose", role: "agent", text: "Hello world", id: "i1" });
  });

  it("replaces a cumulative snapshot instead of concatenating it", () => {
    const state = run([agentMsg("Hel", "i1"), agentMsg("Hello", "i1")]);
    expect(state.items).toHaveLength(1);
    expect(state.items[0]).toMatchObject({ kind: "prose", role: "agent", text: "Hello", id: "i1" });
  });

  it("skips an exact duplicate user echo", () => {
    const state = run([
      { type: "message", role: "user", text: "Fix it", itemId: "u1" },
      { type: "message", role: "user", text: "Fix it", itemId: "u1" },
    ]);
    expect(state.items).toHaveLength(1);
    expect(state.items[0]).toMatchObject({ kind: "prose", role: "user", text: "Fix it" });
  });

  it("starts a new paragraph when a tool run interrupts agent prose", () => {
    const state = run([
      agentMsg("Reading", "i-read"),
      toolCall("c1"),
      agentMsg("Done", "i-done"),
    ]);
    expect(state.items.map((item) => item.kind)).toEqual(["prose", "tool", "prose"]);
    expect(state.items[0]).toMatchObject({ text: "Reading" });
    expect(state.items[2]).toMatchObject({ text: "Done" });
  });

  it("splits one role's stream when the harness changes messageId", () => {
    const state = run([
      agentMsg("First answer", "i-m1", "m1"),
      agentMsg("Second answer", "i-m2", "m2"),
    ]);
    expect(state.items).toHaveLength(2);
    expect(state.items[0]).toMatchObject({ text: "First answer" });
    expect(state.items[1]).toMatchObject({ text: "Second answer" });
  });

  it("updates the same item when messageId lane is unchanged", () => {
    const state = run([agentMsg("Hello again", "i-m1", "m1")]);
    expect(state.items).toHaveLength(1);
    expect(state.items[0]).toMatchObject({ text: "Hello again", id: "i-m1" });
  });

  it("times a call from the client clock the wire does not carry", () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(1_000);
      let state = sessionReducer(initialSessionState, { type: "event", event: toolCall("c1") });
      vi.setSystemTime(3_500);
      state = sessionReducer(state, {
        type: "event",
        event: toolCall("c1", { status: "completed" }),
      });
      // The completing update must not restart the clock, or every call reports
      // the duration of its last status hop instead of its own work.
      expect(activeTool(state)).toMatchObject({ startedAt: 1_000, endedAt: 3_500 });
    } finally {
      vi.useRealTimers();
    }
  });

  it("leaves a running call open-ended until it settles", () => {
    expect(activeTool(run([toolCall("c1", { status: "in_progress" })]))?.endedAt).toBeUndefined();
  });

  it("merges tool_call_update into the open call instead of appending a row", () => {
    const state = run([
      toolCall("c1"),
      toolCall("c1", { status: "completed", title: "", kind: "", locations: [] }),
    ]);
    expect(toolCount(state.items)).toBe(1);
    expect(state.items).toHaveLength(1);
    expect(activeTool(state)).toMatchObject({
      callId: "c1",
      status: "completed",
      title: "Read config",
      kind: "read",
      locations: ["/repo/src/config.ts"],
    });
  });

  it("revises a tool call in place after prose has already arrived", () => {
    // The call belongs where the agent made it. Re-appending on completion put
    // a finished edit below prose that was written after it.
    const state = run([
      toolCall("c1"),
      agentMsg("thinking out loud", "i-thought"),
      toolCall("c1", { status: "completed" }),
    ]);
    expect(state.items.map((item) => item.kind)).toEqual(["tool", "prose"]);
    expect(state.items[0]).toMatchObject({ kind: "tool", call: { status: "completed" } });
  });

  it("carries tool content through to the item", () => {
    const state = run([
      toolCall("c1", {
        kind: "edit",
        status: "completed",
        content: [{ type: "diff", path: "/repo/a.ts", oldText: "a", newText: "b" }],
      }),
    ]);
    expect(state.items[0]).toMatchObject({
      kind: "tool",
      call: { content: [{ type: "diff", path: "/repo/a.ts", oldText: "a", newText: "b" }] },
    });
  });

  it("keeps content an update omits rather than clearing it", () => {
    // A status-only tool_call_update carries no content array. Treating that as
    // an empty one erased the diff the moment the edit finished.
    const state = run([
      toolCall("c1", { content: [{ type: "text", text: "output" }] }),
      toolCall("c1", { status: "completed" }),
    ]);
    expect(state.items[0]).toMatchObject({
      kind: "tool",
      call: { status: "completed", content: [{ type: "text", text: "output" }] },
    });
  });

  it("prefers a still-running tool over a finished one in the head", () => {
    const state = run([
      toolCall("c1", { status: "completed" }),
      toolCall("c2", { status: "in_progress" }),
    ]);
    expect(activeTool(state)?.callId).toBe("c2");
  });

  it("keeps reasoning as its own item rather than folding it into prose", () => {
    const thinking = run([
      { type: "message", role: "thought", text: "the router", itemId: "i-thought" },
    ]);
    expect(thinking.busy).toBe(true);
    expect(thinking.items).toHaveLength(1);
    expect(thinking.items[0]).toMatchObject({ kind: "thought", text: "the router" });

    const answered = sessionReducer(thinking, {
      type: "event",
      event: agentMsg("Found it", "i-found"),
    });
    expect(answered.items.map((item) => item.kind)).toEqual(["thought", "prose"]);
  });

  it("replaces run status in the head rather than appending it to the conversation", () => {
    const state = run([
      { type: "status", state: "running" },
      { type: "status", state: "waiting" },
    ]);
    expect(state.items).toHaveLength(0);
    expect(state.status).toBe("waiting");
    expect(state.statusDetail).toBeNull();
  });

  it("stores human status detail without appending transcript rows", () => {
    const state = run([{ type: "status", state: "running", detail: "Indexing workspace" }]);
    expect(state.items).toHaveLength(0);
    expect(state.status).toBe("running");
    expect(state.statusDetail).toBe("Indexing workspace");
  });

  it("revises the plan in place instead of stacking a copy per update", () => {
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
    expect(state.items.filter((item) => item.kind === "plan")).toHaveLength(1);
    expect(latestPlan(state.items)).toEqual([
      { content: "Read", status: "completed" },
      { content: "Patch", status: "in_progress" },
    ]);
    expect(activePlanStep(latestPlan(state.items))).toBe("Patch");
  });

  it("reads the latest reasoning text for the live head", () => {
    const state = run([
      { type: "message", role: "thought", text: "first", itemId: "t1" },
      { type: "message", role: "thought", text: "second pass", itemId: "t2" },
    ]);
    expect(latestThought(state.items)).toBe("second pass");
  });

  it("formats a one-line thought snippet for the head", () => {
    expect(thoughtSnippet("Checking\n  the router")).toBe("Checking the router");
    expect(thoughtSnippet("x".repeat(130))).toHaveLength(120);
    expect(thoughtSnippet("x".repeat(130)).endsWith("…")).toBe(true);
    const long =
      "Looking for the lifecycle hook in the router module that handles session state transitions";
    expect(thoughtSnippet(long, 60)).toMatch(/\bthat…$/);
    expect(thoughtSnippet(long, 60)).not.toMatch(/…\w/);
  });

  it("tracks context pressure as one current value, not a row per update", () => {
    const state = run([
      { type: "usage", used: 10, size: 100 },
      { type: "usage", used: 40, size: 100 },
    ]);
    expect(state.items).toHaveLength(0);
    expect(state.usage).toEqual({ used: 40, size: 100 });
  });

  it("ignores a usage update with no window to be a fraction of", () => {
    expect(run([{ type: "usage", used: 0, size: 0 }]).usage).toBeNull();
  });

  it("clears the permission head on decided without waiting for permission_resolved (#1018)", () => {
    const state = run([
      { type: "permission_request", requestId: "7", title: "Run tests?", detail: "cargo test" },
    ]);
    expect(state.decision).toEqual({ requestId: "7", title: "Run tests?", detail: "cargo test" });
    expect(state.items).toEqual([
      expect.objectContaining({ kind: "permission", requestId: "7", resolved: false }),
    ]);

    const decided = sessionReducer(state, { type: "decided" });
    expect(decided.decision).toBeNull();
    expect(decided.items[0]).toMatchObject({ kind: "permission", resolved: true });
  });

  it("clears a resurrected decision when permission_resolved replays", () => {
    const state = run([
      { type: "permission_request", requestId: "7", title: "Run tests?" },
      { type: "permission_resolved", requestId: "7", approved: true },
    ]);
    expect(state.decision).toBeNull();
    expect(state.items[0]).toMatchObject({ kind: "permission", resolved: true });
  });

  it("ignores a duplicate permission request after it was answered", () => {
    const answered = sessionReducer(
      run([{ type: "permission_request", requestId: "7", title: "Run tests?" }]),
      { type: "decided" },
    );
    const replayed = sessionReducer(answered, {
      type: "event",
      event: { type: "permission_request", requestId: "7", title: "Run tests?" },
    });
    expect(replayed.decision).toBeNull();
    // Replay must not stack a second marker for the same ask.
    expect(replayed.items.filter((item) => item.kind === "permission")).toHaveLength(1);
  });

  it("tracks the turn: busy on prompt, settled on turn_end", () => {
    const busy = run([{ prompt: "Fix the test" }]);
    expect(busy.busy).toBe(true);
    expect(busy.items).toHaveLength(1);
    expect(busy.items[0]).toMatchObject({ kind: "prose", role: "user", text: "Fix the test" });

    const echoed = sessionReducer(busy, {
      type: "event",
      event: { type: "message", role: "user", text: "Fix the test" },
    });
    expect(echoed.items).toHaveLength(1);

    const settled = sessionReducer(busy, { type: "event", event: { type: "turn_end" } });
    expect(settled.busy).toBe(false);
  });

  it("surfaces a turn that ends in error without an agent response", () => {
    const state = run([{ prompt: "go" }, { type: "turn_end", stopReason: "error" }]);

    expect(state.busy).toBe(false);
    expect(state.items[1]).toMatchObject({
      kind: "note",
      tone: "error",
      text: "The agent stopped without a response. Check the selected model or try again.",
    });
  });

  it("keeps a settled turn's tool calls instead of collapsing them to a summary", () => {
    // The work a turn did is the record of the turn. Folding four calls into
    // "2 read · 1 edit" threw away the diff that made the turn worth reading.
    const state = run([
      toolCall("c1", { kind: "read", status: "completed" }),
      toolCall("c2", { kind: "read", status: "completed" }),
      toolCall("c3", { kind: "edit", status: "completed" }),
      toolCall("c4", { kind: "search", status: "failed" }),
      { type: "turn_end" },
    ]);
    expect(state.busy).toBe(false);
    expect(toolCount(state.items)).toBe(4);
    expect(state.items.map((item) => item.kind)).toEqual(["tool", "tool", "tool", "tool"]);
  });

  it("starts a new paragraph after a turn boundary", () => {
    const state = run([
      agentMsg("first turn", "i-1"),
      { type: "turn_end" },
      agentMsg("second turn", "i-2"),
    ]);
    expect(state.items.map((item) => item.kind)).toEqual(["prose", "prose"]);
  });

  it("ends the turn on error and records it as a conversation note", () => {
    const state = run([{ prompt: "go" }, { type: "error", message: "ACP process exited" }]);
    expect(state.busy).toBe(false);
    expect(state.items[1]).toMatchObject({
      kind: "note",
      tone: "error",
      text: "The agent stopped. It will restart when you reconnect.",
    });
  });

  it("drops unknown artifacts instead of pretty-printing them", () => {
    const empty = run([{ type: "artifact", kind: "x", title: "", body: "" }]);
    expect(empty.items).toHaveLength(0);
    const dumped = run([{ type: "artifact", kind: "x", title: "Modes", body: "{}" }]);
    expect(dumped.items).toHaveLength(0);
  });
});

describe("turn usage stays separate from context usage", () => {
  it("stores turnUsage without writing context usage or zero-filled fields", () => {
    const afterContext = sessionReducer(initialSessionState, {
      type: "event",
      event: { type: "usage", used: 50_000, size: 200_000 },
    });
    expect(afterContext.usage).toEqual({ used: 50_000, size: 200_000 });
    expect(afterContext.turnUsage).toBeNull();

    const afterTurn = sessionReducer(afterContext, {
      type: "event",
      event: { type: "turn_usage", inputTokens: 1200, totalTokens: 1200 },
    });

    expect(afterTurn.usage).toEqual({ used: 50_000, size: 200_000 });
    expect(afterTurn.turnUsage).toEqual({ inputTokens: 1200, totalTokens: 1200 });
    expect(afterTurn.turnUsage).not.toHaveProperty("outputTokens", 0);
    expect(afterTurn.turnUsage).not.toHaveProperty("cacheReadTokens", 0);
    expect(afterTurn.turnUsage).not.toHaveProperty("cacheWriteTokens", 0);
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

  it("maps opaque persist/runtime blocking failures (#962)", () => {
    expect(
      explainAcpError(
        "Cannot block the current thread from within a runtime. This happens because a function attempted to block the current thread while the thread is being used to drive asynchronous tasks.",
      ),
    ).toMatch(/Could not save the selected model/i);
    expect(explainAcpError("session task stopped")).toMatch(/session worker stopped/i);
  });
});

describe("explainOpenFailure", () => {
  it("explains when the task is not session-capable", () => {
    const message = explainOpenFailure({
      agent: "Claude",
      status_explanation: "Running",
      session_capable: false,
    });
    expect(message).toContain("orchestration chat");
    expect(message).toContain("Claude");
    expect(message).not.toContain("Cursor");
  });

  it("passes through the server's own explanation for a capable task", () => {
    expect(
      explainOpenFailure({
        agent: "Cursor",
        status_explanation: "Worktree missing",
        session_capable: true,
      }),
    ).toContain("Worktree missing");
  });

  it("still says something actionable with no detail at all", () => {
    expect(explainOpenFailure(null)).toMatch(/worktree/i);
  });
});
