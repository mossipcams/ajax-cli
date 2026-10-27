import { describe, it, expect, vi } from "vitest";
import type { WebSessionServerEvent } from "./transport/contracts";
import { initialChatSessionReducerState, type ChatSessionEvent, type ChatSessionView } from "./model";
import { projectWireEvent } from "./projectWireInput";
import { reduceChatSession } from "./reducer";

function applySnapshotTranscript(wireEvents: WebSessionServerEvent[]): ChatSessionView {
  const events = wireEvents
    .map(projectWireEvent)
    .filter((event): event is ChatSessionEvent => event !== null);
  return reduceChatSession(initialChatSessionReducerState, { type: "events", events }).view;
}

const toolCall: WebSessionServerEvent = {
  type: "tool_call",
  callId: "c1",
  title: "Read config",
  kind: "read",
  status: "completed",
  locations: ["/repo/src/config.ts"],
};

describe("#1352 superseded-message compaction of snapshot transcripts", () => {
  it("yields the identical conversation for compacted and uncompacted streams", () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(1_000);

      const userMsg: WebSessionServerEvent = { type: "message", role: "user", text: "Question", itemId: "u1" };
      const turnEnd: WebSessionServerEvent = { type: "turn_end", stopReason: "end_turn" };

      const uncompacted: WebSessionServerEvent[] = [
        userMsg,
        { type: "message", role: "agent", text: "He", itemId: "i1" },
        { type: "message", role: "thought", text: "Th", itemId: "i2" },
        { type: "message", role: "agent", text: "Hello", itemId: "i1" },
        { type: "message", role: "thought", text: "Think", itemId: "i2" },
        { type: "message", role: "agent", text: "Hello world", itemId: "i1" },
        toolCall,
        turnEnd,
      ];

      const compacted: WebSessionServerEvent[] = [
        userMsg,
        { type: "message", role: "agent", text: "Hello world", itemId: "i1" },
        { type: "message", role: "thought", text: "Think", itemId: "i2" },
        toolCall,
        turnEnd,
      ];

      const whole = applySnapshotTranscript(uncompacted);
      const compactedView = applySnapshotTranscript(compacted);

      expect(whole.conversation.map((item) => item.id)).toEqual(["u1", "i1", "i2", "e1"]);
      expect(whole.conversation.filter((item) => item.kind === "prose")).toHaveLength(2);

      expect(compactedView.conversation).toEqual(whole.conversation);

      expect(compactedView.conversation.map((item) => item.id)).toEqual(["u1", "i1", "i2", "e1"]);
      expect(compactedView.conversation[1]).toMatchObject({ kind: "prose", role: "agent", text: "Hello world" });
      expect(compactedView.conversation[2]).toMatchObject({ kind: "thought", text: "Think" });
      expect(compactedView.conversation[3]).toMatchObject({ kind: "tool" });
    } finally {
      vi.useRealTimers();
    }
  });
});
