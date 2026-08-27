import { describe, it, expect } from "vitest";
import { failedTurnPromptToRestore } from "./turnProjection";
import type { ConversationItem } from "./model";

describe("failedTurnPromptToRestore", () => {
  it("returns the user prompt when the tail is an error note with no agent answer", () => {
    const conversation: ConversationItem[] = [
      { kind: "prose", id: "u1", role: "user", text: "fix the test" },
      {
        kind: "note",
        id: "e2",
        tone: "error",
        text: "The agent stopped without a response. Check the selected model or try again.",
      },
    ];
    expect(failedTurnPromptToRestore(conversation)).toEqual({
      promptText: "fix the test",
      failureKey: "e2",
    });
  });

  it("returns null when agent prose arrived before the error", () => {
    const conversation: ConversationItem[] = [
      { kind: "prose", id: "u1", role: "user", text: "go" },
      { kind: "prose", id: "a1", role: "agent", text: "Partial answer" },
    ];
    expect(failedTurnPromptToRestore(conversation)).toBeNull();
  });

  it("returns null when the turn did not fail", () => {
    const conversation: ConversationItem[] = [
      { kind: "prose", id: "u1", role: "user", text: "hello" },
      { kind: "note", id: "n1", tone: "info", text: "Stopped" },
    ];
    expect(failedTurnPromptToRestore(conversation)).toBeNull();
  });
});
