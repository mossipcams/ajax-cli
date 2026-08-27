import { describe, it, expect } from "vitest";
import type { ConversationItem } from "../session/public";
import {
  DEFAULT_HISTORY_WINDOW,
  historyWindowStart,
  snapRevealStart,
  turnStartIndices,
} from "./historyWindow";

const userProse = (id: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "user",
  text: id,
});

const agentProse = (id: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "agent",
  text: id,
});

describe("historyWindowStart", () => {
  it("returns 0 when the transcript fits the cap", () => {
    const items = Array.from({ length: 20 }, (_, index) => userProse(`u${index}`));
    expect(historyWindowStart(items, DEFAULT_HISTORY_WINDOW)).toBe(0);
  });

  it("snaps forward to a user-turn boundary instead of opening mid-turn", () => {
    const tool: ConversationItem = {
      kind: "tool",
      id: "t1",
      call: { callId: "c1", title: "Read", kind: "read", status: "completed", locations: [], content: [] },
    };
    const items: ConversationItem[] = [
      ...Array.from({ length: 143 }, (_, index) => agentProse(`p${index}`)),
      userProse("u1"),
      agentProse("a1"),
      tool,
      userProse("u2"),
      agentProse("a2"),
    ];
    expect(historyWindowStart(items, 4)).toBe(146);
    expect(items[146]).toEqual(userProse("u2"));
  });

  it("uses the length cap when no user-turn boundary sits at or after the cut", () => {
    const cap = DEFAULT_HISTORY_WINDOW;
    const items = Array.from({ length: 400 }, (_, index) => agentProse(`a${index}`));
    const start = historyWindowStart(items, cap);
    expect(start).toBe(items.length - cap);
    expect(items.slice(start).length).toBe(cap);
  });
});

describe("turnStartIndices", () => {
  it("includes index zero and each user prompt", () => {
    const items = [agentProse("a0"), userProse("u1"), agentProse("a1"), userProse("u2")];
    expect(turnStartIndices(items)).toEqual([0, 1, 3]);
  });
});

describe("snapRevealStart", () => {
  it("reveals toward an earlier turn boundary", () => {
    const items = [userProse("u1"), agentProse("a1"), userProse("u2"), agentProse("a2")];
    expect(snapRevealStart(items, 3, 1)).toBe(2);
  });

  it("returns zero when already at the top", () => {
    expect(snapRevealStart([userProse("u1")], 0, 50)).toBe(0);
  });

  it("caps reveal at the batch floor when no turn boundary sits in range", () => {
    const items = Array.from({ length: 450 }, (_, index) => agentProse(`a${index}`));
    items.splice(100, 0, userProse("u-far"));
    const windowStart = 401;
    const batch = 50;
    const target = windowStart - batch;
    expect(snapRevealStart(items, windowStart, batch)).toBe(target);
    expect(snapRevealStart(items, windowStart, batch)).not.toBe(100);
  });
});
