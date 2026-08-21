import { describe, it, expect } from "vitest";
import { groupConversationTurns } from "./groupTurns";
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
    expect(turns[0].work.map((item) => item.id)).toEqual(["t1", "x1"]);
    expect(turns[0].agents.map((item) => item.id)).toEqual(["a1"]);
    expect(turns[1].user?.id).toBe("u2");
    expect(turns[1].agents.map((item) => item.id)).toEqual(["a2"]);
  });

  it("keeps preamble items without a user prompt in one turn", () => {
    const items: ConversationItem[] = [
      { kind: "note", id: "n1", tone: "info", text: "Reconnecting" },
      agentProse("a1", "Hello"),
    ];
    const turns = groupConversationTurns(items);
    expect(turns).toHaveLength(1);
    expect(turns[0].user).toBeNull();
    expect(turns[0].other.map((item) => item.id)).toEqual(["n1"]);
    expect(turns[0].agents.map((item) => item.id)).toEqual(["a1"]);
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
    expect(turn.other.map((item) => item.id)).toEqual(["q1"]);
    expect(turn.work.map((item) => item.id)).toEqual(["q2"]);
  });
});
