import { describe, it, expect } from "vitest";
import { flattenTurnItems, groupConversationTurns } from "./sessionTurns";
import type { ConversationItem } from "./sessionThread";

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

  it("keeps preamble items without a user prompt in legacy order", () => {
    const items: ConversationItem[] = [
      { kind: "note", id: "n1", tone: "info", text: "Reconnecting" },
      agentProse("a1", "Hello"),
    ];
    const turns = groupConversationTurns(items);
    expect(turns).toHaveLength(1);
    expect(turns[0].user).toBeNull();
    expect(flattenTurnItems(turns[0]).map((item) => item.id)).toEqual(["n1", "a1"]);
  });
});
