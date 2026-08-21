import { describe, it, expect } from "vitest";
import {
  composerStateAfterFlush,
  flushQueuedFollowUp,
  submitComposerDraft,
} from "./submit";
import { beginStopAndSend, clearQueue, queueFollowUp } from "./composerState";

describe("flushQueuedFollowUp", () => {
  it("does nothing while busy or disconnected", () => {
    const queued = queueFollowUp({ status: "idle" }, "Next");
    expect(flushQueuedFollowUp({ composerState: queued, busy: true, connected: true })).toEqual({
      state: queued,
      intents: [],
    });
    expect(flushQueuedFollowUp({ composerState: queued, busy: false, connected: false })).toEqual({
      state: queued,
      intents: [],
    });
  });

  it("plans mark_stopped and send_prompt when stopping after cancel", () => {
    const stopping = beginStopAndSend(queueFollowUp({ status: "idle" }, "Next"));
    expect(flushQueuedFollowUp({ composerState: stopping, busy: false, connected: true })).toEqual({
      state: stopping,
      intents: [{ type: "mark_stopped" }, { type: "send_prompt", text: "Next" }],
    });
  });

  it("plans only send_prompt when a queued follow-up outlives a normal turn end", () => {
    const queued = queueFollowUp({ status: "idle" }, "Next");
    expect(flushQueuedFollowUp({ composerState: queued, busy: false, connected: true })).toEqual({
      state: queued,
      intents: [{ type: "send_prompt", text: "Next" }],
    });
  });

  it("clears the queue only after a successful send", () => {
    const stopping = beginStopAndSend(queueFollowUp({ status: "idle" }, "Next"));
    expect(composerStateAfterFlush(stopping, true)).toEqual(clearQueue(stopping));
    expect(composerStateAfterFlush(stopping, false)).toBe(stopping);
  });
});

describe("submitComposerDraft", () => {
  it("queues while busy and stop-and-sends on a second Enter", () => {
    expect(
      submitComposerDraft({
        connected: true,
        busy: true,
        draft: "Next",
        composerState: { status: "idle" },
      }),
    ).toEqual({ action: "queue", text: "Next", clearDraft: true });

    const queued = queueFollowUp({ status: "idle" }, "Next");
    expect(
      submitComposerDraft({
        connected: true,
        busy: true,
        draft: "",
        composerState: queued,
      }),
    ).toEqual({ action: "stop_and_send", sendCancel: true, clearDraft: true });
  });
});
