import { describe, it, expect } from "vitest";
import { groupConversationTurns, type TurnRow } from "./groupTurns";
import type { ConversationItem } from "../session/public";

const userProse = (id: string, text: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "user",
  text,
});

const agentProse = (id: string, text: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "agent",
  text,
});

const rowIds = (turn: { rows: { id: string }[] }) => turn.rows.map((row) => row.id);

const workIds = (turn: { rows: TurnRow[] }) =>
  turn.rows.flatMap((row) => (row.kind === "work" ? row.items.map((item) => item.id) : []));

const looseIds = (turn: { rows: TurnRow[] }) =>
  turn.rows.flatMap((row) => (row.kind === "item" ? [row.item.id] : []));

describe("groupConversationTurns", () => {
  it("groups items from a user prompt through the next user prompt", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix login"),
      { kind: "thought", id: "t1", text: "Checking auth" },
      { kind: "tool", id: "x1", call: { callId: "c1", title: "Read", kind: "read", status: "completed", locations: [], content: [] } },
      agentProse("a1", "Updated the handler."),
      userProse("u2", "Run tests"),
      agentProse("a2", "All green."),
    ];

    const turns = groupConversationTurns(items);
    expect(turns).toHaveLength(2);
    expect(turns[0].user?.id).toBe("u1");
    expect(workIds(turns[0])).toEqual(["t1", "x1"]);
    expect(looseIds(turns[0])).toEqual(["a1"]);
    expect(turns[1].user?.id).toBe("u2");
    expect(looseIds(turns[1])).toEqual(["a2"]);
  });

  // #1042: the transcript hoisted every work item above every thing the agent said, so
  // an agent that spoke, worked, then answered read as work-then-two-answers and
  // the reader lost the causal order of its own turn.
  it("keeps prose and work in the order they arrived", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix login"),
      agentProse("a1", "Let me look at the handler."),
      { kind: "thought", id: "t1", text: "Checking auth" },
      { kind: "tool", id: "x1", call: { callId: "c1", title: "Read", kind: "read", status: "completed", locations: [], content: [] } },
      agentProse("a2", "Updated the handler."),
      { kind: "tool", id: "x2", call: { callId: "c2", title: "Test", kind: "execute", status: "completed", locations: [], content: [] } },
      agentProse("a3", "Tests pass."),
    ];

    const [turn] = groupConversationTurns(items);
    expect(rowIds(turn)).toEqual(["a1", "work:t1", "a2", "work:x2", "a3"]);
    // Adjacent work still collapses into one disclosure; prose ends the run.
    expect(turn.rows[1]).toMatchObject({ kind: "work" });
    expect(workIds(turn)).toEqual(["t1", "x1", "x2"]);
  });

  it("keeps preamble items without a user prompt in one turn", () => {
    const items: ConversationItem[] = [
      { kind: "note", id: "n1", tone: "info", text: "Reconnecting" },
      agentProse("a1", "Hello"),
    ];
    const turns = groupConversationTurns(items);
    expect(turns).toHaveLength(1);
    expect(turns[0].user).toBeNull();
    expect(looseIds(turns[0])).toEqual(["n1", "a1"]);
  });

  // An ask the operator still owes an answer to is an action, so it stays in
  // the conversation; once answered it is history and joins the timeline.
  it("routes permissions by whether they still need an answer", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Clean up"),
      { kind: "permission", id: "q1", requestId: "r1", title: "Delete?", resolved: false },
      { kind: "permission", id: "q2", requestId: "r2", title: "Run tests?", resolved: true },
    ];
    const [turn] = groupConversationTurns(items);
    expect(looseIds(turn)).toEqual(["q1"]);
    expect(workIds(turn)).toEqual(["q2"]);
  });

  it("routes elicitations by whether they still need an answer", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Deploy"),
      { kind: "elicitation", id: "e1", requestId: "r1", message: "Pick env", resolved: false },
      { kind: "elicitation", id: "e2", requestId: "r2", message: "Confirmed", resolved: true },
    ];
    const [turn] = groupConversationTurns(items);
    expect(looseIds(turn)).toEqual(["e1"]);
    expect(workIds(turn)).toEqual(["e2"]);
  });
});
