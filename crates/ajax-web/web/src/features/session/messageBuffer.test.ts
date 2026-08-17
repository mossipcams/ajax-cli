import { describe, expect, it } from "vitest";
import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
import { MessageBuffer } from "./messageBuffer";

describe("MessageBuffer (render batching only)", () => {
  it("holds streamed updates until turn_end, then flushes the latest full text", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "The ", itemId: "i1" });
    buffer.push({ type: "message", role: "agent", text: "The bug is here", itemId: "i1" });
    expect(dispatched).toEqual([]);
    buffer.push({ type: "turn_end", stopReason: "end_turn" });

    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "The bug is here", itemId: "i1" },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    buffer.dispose();
  });

  it("flushes pending prose before a tool call", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "I'll edit ", itemId: "i1" });
    buffer.push({
      type: "tool_call",
      callId: "c1",
      title: "edit",
      kind: "edit",
      status: "in_progress",
      locations: [],
      content: [],
    });

    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "I'll edit ", itemId: "i1" },
      {
        type: "tool_call",
        callId: "c1",
        title: "edit",
        kind: "edit",
        status: "in_progress",
        locations: [],
        content: [],
      },
    ]);
    buffer.dispose();
  });

  it("passes non-streamed events through immediately", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "user", text: "do the thing" });
    expect(dispatched).toEqual([{ type: "message", role: "user", text: "do the thing" }]);
    buffer.dispose();
  });

  it("dispose drops pending text without dispatching", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));
    buffer.push({ type: "message", role: "agent", text: "in flight", itemId: "i1" });
    buffer.dispose();
    expect(dispatched).toEqual([]);
  });
});
