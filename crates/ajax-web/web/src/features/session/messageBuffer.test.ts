import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
import { MessageBuffer } from "./messageBuffer";

type FrameCallback = (time: number) => void;

let frameQueue: FrameCallback[] = [];

function flushFrame(): void {
  const callbacks = frameQueue.splice(0);
  for (const callback of callbacks) {
    callback(0);
  }
}

beforeEach(() => {
  frameQueue = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameCallback) => {
    frameQueue.push(callback);
    return frameQueue.length;
  });
  vi.stubGlobal("cancelAnimationFrame", () => {});
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("MessageBuffer (render batching only)", () => {
  it("rAF-coalesces streamed updates to the latest full text per itemId", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "The ", itemId: "i1" });
    buffer.push({ type: "message", role: "agent", text: "The bug is here", itemId: "i1" });
    expect(dispatched).toEqual([]);

    flushFrame();
    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "The bug is here", itemId: "i1" },
    ]);

    buffer.push({ type: "turn_end", stopReason: "end_turn" });
    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "The bug is here", itemId: "i1" },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    buffer.dispose();
  });

  it("dispatches thought updates during the turn, not only at turn_end", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "thought", text: "Checking", itemId: "t1" });
    flushFrame();
    expect(dispatched).toEqual([
      { type: "message", role: "thought", text: "Checking", itemId: "t1" },
    ]);

    buffer.push({ type: "message", role: "thought", text: "Checking files", itemId: "t1" });
    flushFrame();
    expect(dispatched).toEqual([
      { type: "message", role: "thought", text: "Checking", itemId: "t1" },
      { type: "message", role: "thought", text: "Checking files", itemId: "t1" },
    ]);
    buffer.dispose();
  });

  it("flushes pending prose before a tool call", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "I'll edit ", itemId: "i1" });
    flushFrame();
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
    flushFrame();
    expect(dispatched).toEqual([]);
  });
});
