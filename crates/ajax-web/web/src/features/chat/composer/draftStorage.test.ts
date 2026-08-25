import { afterEach, describe, expect, it } from "vitest";
import {
  clearComposerDraft,
  clearComposerPresentationState,
  clearComposerQueue,
  readComposerDraft,
  readComposerQueue,
  writeComposerDraft,
  writeComposerQueue,
} from "./draftStorage";

afterEach(() => {
  localStorage.clear();
});

describe("draftStorage", () => {
  it("round-trips unsent composer text per handle in localStorage", () => {
    writeComposerDraft("web/fix-login", "still typing");
    expect(readComposerDraft("web/fix-login")).toBe("still typing");
    expect(localStorage.getItem("ajax.web.session.composer.draft.web%2Ffix-login")).toBe(
      "still typing",
    );
    expect(sessionStorage.length).toBe(0);
  });

  it("isolates drafts by task handle", () => {
    writeComposerDraft("web/task-a", "alpha");
    writeComposerDraft("web/task-b", "beta");
    expect(readComposerDraft("web/task-a")).toBe("alpha");
    expect(readComposerDraft("web/task-b")).toBe("beta");
  });

  it("clears stored draft when text is empty", () => {
    writeComposerDraft("web/fix-login", "draft");
    writeComposerDraft("web/fix-login", "");
    expect(readComposerDraft("web/fix-login")).toBe("");
    expect(localStorage.length).toBe(0);
  });

  it("clearComposerDraft removes the stored entry", () => {
    writeComposerDraft("web/fix-login", "draft");
    clearComposerDraft("web/fix-login");
    expect(readComposerDraft("web/fix-login")).toBe("");
  });

  it("round-trips one queued follow-up per handle", () => {
    writeComposerQueue("web/fix-login", { status: "idle" });
    writeComposerQueue("web/fix-login", {
      status: "queued",
      text: "follow up after turn",
    });
    expect(readComposerQueue("web/fix-login")).toEqual({
      status: "queued",
      text: "follow up after turn",
    });
  });

  it("restores stopping as queued", () => {
    writeComposerQueue("web/fix-login", {
      status: "stopping",
      text: "stop and send this",
    });
    expect(readComposerQueue("web/fix-login")).toEqual({
      status: "queued",
      text: "stop and send this",
    });
  });

  it("persists serializable queued content blocks", () => {
    const blocks = [{ type: "image" as const, data: "abc", mimeType: "image/jpeg" }];
    writeComposerQueue("web/x", {
      status: "queued",
      text: "with image",
      contentBlocks: blocks,
    });
    expect(readComposerQueue("web/x")).toEqual({
      status: "queued",
      text: "with image",
      contentBlocks: blocks,
    });
  });

  it("clearComposerPresentationState removes draft and queue keys", () => {
    writeComposerDraft("web/fix-login", "draft");
    writeComposerQueue("web/fix-login", { status: "queued", text: "queued" });
    clearComposerPresentationState("web/fix-login");
    expect(readComposerDraft("web/fix-login")).toBe("");
    expect(readComposerQueue("web/fix-login")).toEqual({ status: "idle" });
    expect(localStorage.length).toBe(0);
  });

  it("clearComposerQueue removes the stored entry", () => {
    writeComposerQueue("web/fix-login", { status: "queued", text: "queued" });
    clearComposerQueue("web/fix-login");
    expect(readComposerQueue("web/fix-login")).toEqual({ status: "idle" });
  });
});
