import { describe, it, expect } from "vitest";
import {
  assertComposerState,
  beginStopAndSend,
  clearQueue,
  composerIsStopping,
  composerQueuedContentBlocks,
  composerQueuedText,
  queueFollowUp,
  restoreQueuedDraft,
} from "./composerState";

describe("composerState", () => {
  it("cannot represent stopping without queued text or attachments", () => {
    expect(() => assertComposerState({ status: "stopping", text: "   " })).toThrow(
      "ComposerState stopping requires queued text or attachments",
    );
  });

  it("tracks one editable follow-up", () => {
    const queued = queueFollowUp({ status: "idle" }, "Next");
    expect(composerQueuedText(queued)).toBe("Next");
    expect(composerIsStopping(queued)).toBe(false);
  });

  it("enters stopping only from a queued follow-up", () => {
    const queued = queueFollowUp({ status: "idle" }, "Next");
    const stopping = beginStopAndSend(queued);
    expect(composerIsStopping(stopping)).toBe(true);
    expect(composerQueuedText(stopping)).toBe("Next");
    expect(beginStopAndSend({ status: "idle" })).toEqual({ status: "idle" });
  });

  it("restores queued text and attachments into the draft when edited", () => {
    const blocks = [{ type: "image" as const, data: "aGVsbG8=", mimeType: "image/png" }];
    const restored = restoreQueuedDraft(queueFollowUp({ status: "idle" }, "Next", blocks));
    expect(restored).toEqual({
      state: { status: "idle" },
      draft: "Next",
      contentBlocks: blocks,
    });
    expect(composerQueuedContentBlocks(restored!.state)).toBeUndefined();
    expect(restoreQueuedDraft(clearQueue({ status: "idle" }))).toBeNull();
  });
});
