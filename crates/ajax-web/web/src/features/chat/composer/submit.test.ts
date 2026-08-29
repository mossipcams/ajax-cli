import { describe, it, expect } from "vitest";
import {
  applySubmitResult,
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
    const blocks = [{ type: "image" as const, data: "aGVsbG8=", mimeType: "image/png" }];
    const stopping = beginStopAndSend(queueFollowUp({ status: "idle" }, "Next", blocks));
    expect(flushQueuedFollowUp({ composerState: stopping, busy: false, connected: true })).toEqual({
      state: stopping,
      intents: [
        { type: "mark_stopped" },
        { type: "send_prompt", text: "Next", contentBlocks: blocks },
      ],
    });
  });

  it("plans only send_prompt when a queued follow-up outlives a normal turn end", () => {
    const blocks = [{ type: "resource" as const, uri: "file:///notes.txt", text: "hello" }];
    const queued = queueFollowUp({ status: "idle" }, "Next", blocks);
    expect(flushQueuedFollowUp({ composerState: queued, busy: false, connected: true })).toEqual({
      state: queued,
      intents: [{ type: "send_prompt", text: "Next", contentBlocks: blocks }],
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

  // ajax-cli#1081: typing while a follow-up is queued replaces it instead of cancelling.
  it("updates the queued follow-up when new text is submitted (#1081)", () => {
    const queued = queueFollowUp({ status: "idle" }, "A");
    expect(
      submitComposerDraft({
        connected: true,
        busy: true,
        draft: "B",
        composerState: queued,
      }),
    ).toEqual({ action: "update_queue", text: "B", clearDraft: true });
  });
});

describe("applySubmitResult", () => {
  const blocks = [{ type: "image" as const, data: "aGVsbG8=", mimeType: "image/png" }];

  function baseArgs(overrides: Partial<Parameters<typeof applySubmitResult>[2]> = {}) {
    return {
      connected: true,
      busy: true,
      draft: "",
      composerState: { status: "idle" } as const,
      ...overrides,
    };
  }

  // ajax-cli#1081: replace queued text without session/cancel.
  it("replaces queued follow-up text without changing stopping state (#1081)", () => {
    const queued = queueFollowUp({ status: "idle" }, "A");
    const next = applySubmitResult(
      { action: "update_queue", text: "B", clearDraft: true },
      queued,
      baseArgs({ composerState: queued }),
    );

    expect(next).toEqual(queueFollowUp({ status: "idle" }, "B"));
  });

  // ajax-cli#1081: empty submit while queued still enters stopping state.
  it("enters stopping on empty submit while queued and busy (#1081)", () => {
    const queued = queueFollowUp({ status: "idle" }, "A");
    const next = applySubmitResult(
      { action: "stop_and_send", sendCancel: true, clearDraft: true },
      queued,
      baseArgs({ composerState: queued }),
    );

    expect(next).toEqual(beginStopAndSend(queued));
  });

  it("queues draft attachments on first queue while busy", () => {
    const next = applySubmitResult(
      { action: "queue", text: "Next", clearDraft: true },
      { status: "idle" },
      baseArgs({ contentBlocks: blocks }),
    );

    expect(next).toEqual(queueFollowUp({ status: "idle" }, "Next", blocks));
  });

  // ajax-cli#1081: naive routing must not drop queued attachments when text is replaced.
  it("preserves queued attachments when replacing follow-up text (#1081)", () => {
    const queued = queueFollowUp({ status: "idle" }, "A", blocks);
    const next = applySubmitResult(
      { action: "update_queue", text: "B", clearDraft: true },
      queued,
      baseArgs({ composerState: queued }),
    );

    expect(next).toEqual(queueFollowUp({ status: "idle" }, "B", blocks));
  });
});
