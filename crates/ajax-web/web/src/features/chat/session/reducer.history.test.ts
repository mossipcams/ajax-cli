import { describe, it, expect, vi } from "vitest";
import type { WebSessionServerEvent } from "./transport/contracts";
import { initialChatSessionReducerState, type ChatSessionEvent, type ChatSessionView } from "./model";
import { projectWireEvent } from "./projectWireInput";
import { reduceChatSession, type ChatSessionReducerState } from "./reducer";


function reduceWire(
  state: ChatSessionReducerState,
  event: WebSessionServerEvent,
): ChatSessionReducerState {
  const projected = projectWireEvent(event);
  if (!projected) return state;
  return reduceChatSession(state, { type: "event", event: projected });
}

function asReducer(view: ChatSessionView, seq = 0): ChatSessionReducerState {
  return { view, seq };
}

function run(events: (WebSessionServerEvent | { prompt: string })[]): ChatSessionView {
  const final = events.reduce<ChatSessionReducerState>(
    (state, event) =>
      "prompt" in event
        ? reduceChatSession(state, { type: "prompt", text: event.prompt })
        : reduceWire(state, event),
    initialChatSessionReducerState,
  );
  return final.view;
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

describe("server history bounds and older-page prepend", () => {
  const eventsView = (events: ChatSessionEvent[]): ChatSessionView =>
    reduceChatSession(initialChatSessionReducerState, { type: "events", events }).view;

  const event = (e: ChatSessionEvent) => ({ type: "event" as const, event: e });

  it("history_bounds records the server's remaining history window", () => {
    let state = reduceChatSession(initialChatSessionReducerState, event({ type: "history_bounds", from: 5, floor: 2 }));
    expect(state.view.serverHistory).toEqual({ from: 5, floor: 2 });
    const withEarlier = state.view.serverHistory!;
    expect(withEarlier.from > withEarlier.floor).toBe(true);

    state = reduceChatSession(state, event({ type: "history_bounds", from: 2, floor: 2 }));
    expect(state.view.serverHistory).toEqual({ from: 2, floor: 2 });
    const atFloor = state.view.serverHistory!;
    expect(atFloor.from > atFloor.floor).toBe(false);
  });

  it("a ready event with reset true clears serverHistory", () => {
    let state = reduceChatSession(initialChatSessionReducerState, event({ type: "history_bounds", from: 9, floor: 0 }));
    state = reduceChatSession(state, event({ type: "session_ready", busy: false, reset: true }));
    expect(state.view.serverHistory).toBeUndefined();

    state = reduceChatSession(initialChatSessionReducerState, event({ type: "history_bounds", from: 9, floor: 0 }));
    state = reduceChatSession(state, event({ type: "session_ready", busy: false, reset: false }));
    expect(state.view.serverHistory).toEqual({ from: 9, floor: 0 });
  });

  it("an empty prepend only moves serverHistory.from and keeps the floor", () => {
    const prior = eventsView([{ type: "agent_message", text: "Answer", itemId: "a1" }]);
    let state = reduceChatSession(asReducer(prior), event({ type: "history_bounds", from: 10, floor: 3 }));
    const beforeState = state;
    const before = state.view.conversation;

    state = reduceChatSession(state, event({ type: "prepend", events: [], from: 4 }));
    expect(state.view.conversation).toEqual(before);
    expect(state.view.serverHistory).toEqual({ from: 4, floor: 3 });
    expect(state).toEqual({
      ...beforeState,
      view: { ...beforeState.view, serverHistory: { from: 4, floor: 3 } },
    });
  });

  it("a prepend without prior bounds initializes the window at its own cursor", () => {
    const state = reduceChatSession(initialChatSessionReducerState, event({ type: "prepend", events: [], from: 7 }));
    expect(state.view.serverHistory).toEqual({ from: 7, floor: 7 });
    expect(state.view.conversation).toHaveLength(0);
  });

  it("prepend leaves turn, permission, elicitation, usage, model, and status untouched", () => {
    const prior = eventsView([
      { type: "agent_message", text: "partial", itemId: "a-live" },
      { type: "permission_request", requestId: "p9", title: "Run tests?", detail: "cargo test" },
      { type: "context_usage", used: 40, size: 100 },
      { type: "turn_usage", usage: { inputTokens: 12, outputTokens: 7, totalTokens: 19 } },
      { type: "acp_status", state: "connected", detail: "current connection" },
    ]);
    prior.elicitation = {
      decision: { requestId: "e-live", message: "Choose", schema: {}, fields: [] },
      resolvedIds: ["e-done"],
    };
    prior.model = {
      confirmedModel: "current-model",
      sessionTitle: "Current title",
      configOptions: [],
      availableCommands: [],
    };

    const state = reduceChatSession(asReducer(prior), event({
      type: "prepend",
      events: [
        { type: "agent_message", text: "older answer", itemId: "a-old" },
        { type: "context_usage", used: 1, size: 10 },
        { type: "turn_usage", usage: { totalTokens: 1 } },
        { type: "acp_status", state: "disconnected", detail: "old connection" },
        { type: "turn_end" },
      ],
      from: 1,
    }));

    expect(state.view.turn).toEqual(prior.turn);
    expect(state.view.permission).toEqual(prior.permission);
    expect(state.view.elicitation).toEqual(prior.elicitation);
    expect(state.view.usage).toEqual(prior.usage);
    expect(state.view.model).toEqual(prior.model);
    expect(state.view.status).toEqual(prior.status);
    const ids = state.view.conversation.map((item) => item.id);
    expect(ids[0]).toBe("a-old");
    expect(ids.slice(1)).toEqual(prior.conversation.map((item) => item.id));
    expect(state).toEqual({
      ...asReducer(prior),
      view: { ...prior, conversation: state.view.conversation, serverHistory: state.view.serverHistory },
    });
    prior.conversation.forEach((item, index) => {
      expect(state.view.conversation.at(index - prior.conversation.length)).toBe(item);
    });
  });

  it("offsets a colliding generated id on the prepended page without renumbering live items", () => {
    const prior = run([{ type: "message", role: "agent", text: "live note" }]);
    expect(prior.conversation[0].id).toBe("e1");

    // The older page's host note also projects to generated id e1 from an empty state.
    const state = reduceChatSession(asReducer(prior), event({
      type: "prepend",
      events: [{ type: "host_note", text: "older" }],
      from: 1,
    }));

    const ids = state.view.conversation.map((item) => item.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids[0]).not.toBe("e1");
    expect(state.view.conversation.at(-1)).toMatchObject({ id: "e1", text: "live note" });
  });

  it("tail + prepend on a user-message boundary deep-equals the whole transcript", () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(1_000);
      const older: WebSessionServerEvent[] = [
        { type: "message", role: "user", text: "old question", itemId: "u-old" },
        agentMsg("old part one...", "a-old"),
        agentMsg("old answer", "a-old"),
        toolCall("c-old", { status: "in_progress" }),
        toolCall("c-old", { status: "completed" }),
        agentMsg("after the tool", "a-old2"),
        { type: "turn_end", stopReason: "end_turn" },
      ];
      const turns: WebSessionServerEvent[][] = Array.from({ length: 4 }, (_, index) => [
        { type: "message", role: "user", text: `question ${index}`, itemId: `u-${index}` },
        agentMsg(`partial ${index}`, `a-${index}`),
        agentMsg(`answer ${index}`, `a-${index}`),
        { type: "turn_end", stopReason: "end_turn" },
      ]);
      for (let split = 0; split < turns.length; split += 1) {
        const prefix = [...older, ...turns.slice(0, split).flat()];
        const tail = turns.slice(split).flat();
        const whole = run([...prefix, ...tail]);
        const events = prefix.map(projectWireEvent).filter((e): e is ChatSessionEvent => e !== null);
        expect(events).toHaveLength(prefix.length);
        const state = reduceChatSession(asReducer(run(tail)), event({
          type: "prepend",
          events,
          from: 1,
        }));

        expect(state.view.conversation).toEqual(whole.conversation);
        expect(state.view.conversation.find((item) => item.id === "a-old")).toMatchObject({
          text: "old answer",
        });
        expect(state.view.conversation.filter((item) => item.kind === "tool")).toHaveLength(1);
      }
    } finally {
      vi.useRealTimers();
    }
  });

  it.each([undefined, false, true])("wire ready reset clears bounds with busy=%s", (busy) => {
    const state = reduceChatSession(initialChatSessionReducerState, event({
      type: "history_bounds", from: 9, floor: 2,
    }));
    expect(reduceWire(state, { type: "ready", reset: true, busy }).view.serverHistory).toBeUndefined();
    expect(reduceWire(state, { type: "ready", reset: false, busy }).view.serverHistory)
      .toEqual({ from: 9, floor: 2 });
    expect(reduceWire(state, { type: "ready", busy }).view.serverHistory)
      .toEqual({ from: 9, floor: 2 });
  });
});
