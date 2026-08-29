import { describe, it, expect } from "vitest";
import type { ConversationItem } from "../session/public";
import { activitySummary } from "./activitySummary";
import { currentOperation } from "./currentOperation";

describe("activitySummary", () => {
  it("names read, edit, and execute work separately", () => {
    const items: ConversationItem[] = [
      {
        kind: "tool",
        id: "r1",
        call: {
          callId: "r1",
          title: "Read",
          kind: "read",
          status: "completed",
          locations: ["/a"],
          content: [],
        },
      },
      {
        kind: "tool",
        id: "e1",
        call: {
          callId: "e1",
          title: "Edit",
          kind: "edit",
          status: "completed",
          locations: ["/b"],
          content: [],
        },
      },
      {
        kind: "tool",
        id: "x1",
        call: {
          callId: "x1",
          title: "Run",
          kind: "execute",
          status: "completed",
          locations: [],
          content: [],
        },
      },
    ];
    expect(activitySummary(items)).toBe("Read 1 file · edited 1 file · ran 1 command");
  });
});

describe("currentOperation", () => {
  it("prefers the in-flight tool call", () => {
    const items: ConversationItem[] = [
      {
        kind: "tool",
        id: "e1",
        call: {
          callId: "e1",
          title: "cargo test",
          kind: "execute",
          status: "in_progress",
          locations: [],
          content: [],
        },
      },
    ];
    expect(currentOperation(items)).toBe("Running cargo test…");
  });
});
