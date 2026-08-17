// Regression for issue #904: the assistant response must render as paragraphs,
// not as a token-by-token stream. Typewriter-free — the buffer holds streamed
// chunks for the turn and flushes only at a boundary (turn_end, before a
// non-text event, on ready). These tests fail on the unbuffered behavior (each
// chunk dispatches immediately) and on the phrase-burst behavior (chunks flush
// on a timer/char threshold before the turn ends).

import { describe, expect, it } from "vitest";
import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
import { MessageBuffer } from "./messageBuffer";

describe("MessageBuffer (typewriter-free)", () => {
  it("holds streamed chunks until turn_end, then flushes one combined message", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    // Four token-sized chunks arrive back-to-back. Unbuffered, each would
    // dispatch and the response would read word-by-word; held, they become one
    // message that renders as finished paragraphs at turn_end.
    buffer.push({ type: "message", role: "agent", text: "The " });
    buffer.push({ type: "message", role: "agent", text: "The bug " });
    buffer.push({ type: "message", role: "agent", text: "The bug is " });
    buffer.push({ type: "message", role: "agent", text: "The bug is here" });

    // Nothing reaches the reducer while the turn is in flight.
    expect(dispatched).toEqual([]);

    buffer.push({ type: "turn_end", stopReason: "end_turn" });

    // The full prose flushes first, then the turn_end — order preserved so the
    // transcript never shows the turn ending above its own last words.
    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "The bug is here" },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    buffer.dispose();
  });

  it("flushes pending prose before a tool call so the call never jumps above it", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "I'll edit " });
    buffer.push({
      type: "tool_call",
      callId: "c1",
      title: "edit",
      kind: "edit",
      status: "in_progress",
      locations: [],
      content: [],
    });

    // The prose segment that preceded the tool call is a complete utterance;
    // it flushes as a paragraph, then the tool call lands below it.
    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "I'll edit " },
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

  it("treats a cumulative snapshot as the full text, not a delta appended twice", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "Hello" });
    // The harness resends the whole message so far plus a word.
    buffer.push({ type: "message", role: "agent", text: "Hello world" });
    buffer.push({ type: "turn_end", stopReason: "end_turn" });

    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "Hello world" },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    buffer.dispose();
  });

  it("passes non-streamed events through immediately, flushing pending prose first", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    // User echoes are not streamed responses: they pass through, after any
    // pending agent prose so the user turn lands in order.
    buffer.push({ type: "message", role: "agent", text: "prior " });
    buffer.push({ type: "message", role: "user", text: "do the thing" });

    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "prior " },
      { type: "message", role: "user", text: "do the thing" },
    ]);

    // A plan after in-flight agent prose flushes that prose first.
    buffer.push({ type: "message", role: "agent", text: "planning " });
    buffer.push({ type: "plan", entries: [{ content: "step", status: "in_progress" }] });

    expect(dispatched.slice(2)).toEqual([
      { type: "message", role: "agent", text: "planning " },
      { type: "plan", entries: [{ content: "step", status: "in_progress" }] },
    ]);
    buffer.dispose();
  });

  it("keeps distinct messages (different messageId) on separate lanes, flushed in order", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "first ", messageId: "m1" });
    buffer.push({ type: "message", role: "agent", text: "second ", messageId: "m2" });
    buffer.push({ type: "turn_end", stopReason: "end_turn" });

    expect(dispatched).toEqual([
      { type: "message", role: "agent", text: "first ", messageId: "m1" },
      { type: "message", role: "agent", text: "second ", messageId: "m2" },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    buffer.dispose();
  });

  it("dispose drops pending text without dispatching", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "in flight" });
    buffer.dispose();

    expect(dispatched).toEqual([]);
  });

  it("buffers reasoning (thought) on its own lane until a boundary", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "thought", text: "considering " });
    buffer.push({ type: "message", role: "thought", text: "considering the " });
    buffer.push({ type: "turn_end", stopReason: "end_turn" });

    expect(dispatched).toEqual([
      { type: "message", role: "thought", text: "considering the " },
      { type: "turn_end", stopReason: "end_turn" },
    ]);
    buffer.dispose();
  });

  it("flushAll exposes pending text synchronously (replayed history on ready)", () => {
    const dispatched: WebSessionServerEvent[] = [];
    const buffer = new MessageBuffer((event) => dispatched.push(event));

    buffer.push({ type: "message", role: "agent", text: "replayed answer" });
    buffer.flushAll();

    expect(dispatched).toEqual([{ type: "message", role: "agent", text: "replayed answer" }]);
    buffer.dispose();
  });
});
