import { describe, it, expect } from "vitest";
import {
  applySubmitResult,
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

  it("plans mark_stopped and clears queue when stopping after cancel", () => {
    const blocks = [{ type: "image" as const, data: "aGVsbG8=", mimeType: "image/png" }];
    const stopping = beginStopAndSend(queueFollowUp({ status: "idle" }, "Next", blocks));
    expect(flushQueuedFollowUp({ composerState: stopping, busy: false, connected: true })).toEqual({
      state: clearQueue(stopping),
      intents: [{ type: "mark_stopped" }],
    });
  });

  it("clears the queue when a staged follow-up outlives a normal turn end", () => {
    const blocks = [{ type: "resource" as const, uri: "file:///notes.txt", text: "hello" }];
    const queued = queueFollowUp({ status: "idle" }, "Next", blocks);
    expect(flushQueuedFollowUp({ composerState: queued, busy: false, connected: true })).toEqual({
      state: clearQueue(queued),
      intents: [],
    });
  });
});

describe("submitComposerDraft", () => {
  const blocks = [{ type: "image" as const, data: "aGVsbG8=", mimeType: "image/png" }];

  it("sends image-only drafts when idle", () => {
    expect(
      submitComposerDraft({
        connected: true,
        busy: false,
        draft: "",
        composerState: { status: "idle" },
        contentBlocks: blocks,
      }),
    ).toEqual({ action: "send", text: "", clearDraft: true });
  });

  it("queues image-only drafts while busy", () => {
    expect(
      submitComposerDraft({
        connected: true,
        busy: true,
        draft: "",
        composerState: { status: "idle" },
        contentBlocks: blocks,
      }),
    ).toEqual({ action: "queue", text: "", clearDraft: true });
  });

  it("rejects truly empty drafts", () => {
    expect(
      submitComposerDraft({
        connected: true,
        busy: false,
        draft: "   ",
        composerState: { status: "idle" },
        contentBlocks: [],
      }),
    ).toEqual({ action: "none" });
  });

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

  it("updates the queued follow-up with attachment-only drafts while busy", () => {
    const queued = queueFollowUp({ status: "idle" }, "A");
    expect(
      submitComposerDraft({
        connected: true,
        busy: true,
        draft: "",
        composerState: queued,
        contentBlocks: blocks,
      }),
    ).toEqual({ action: "update_queue", text: "", clearDraft: true });
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

  it("replaces queued follow-up with attachment-only drafts (#1081)", () => {
    const newBlocks = [{ type: "image" as const, data: "d29ybGQ=", mimeType: "image/jpeg" }];
    const queued = queueFollowUp({ status: "idle" }, "A");
    const next = applySubmitResult(
      { action: "update_queue", text: "", clearDraft: true },
      queued,
      baseArgs({ composerState: queued, contentBlocks: newBlocks }),
    );

    expect(next).toEqual(queueFollowUp({ status: "idle" }, "", newBlocks));
  });
});
