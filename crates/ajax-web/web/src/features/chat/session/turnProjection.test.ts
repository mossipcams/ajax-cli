import { describe, it, expect } from "vitest";
import {
  applyOptimisticPrompt,
  applyTurnEvent,
  failedTurnPromptToRestore,
} from "./turnProjection";
import { initialChatSessionReducerState, type ConversationItem } from "./model";

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

describe("optimistic prompt acknowledgement", () => {
  it("renders an optimistic prompt as pending until its prompt_accepted arrives", () => {
    const optimistic = applyOptimisticPrompt(initialChatSessionReducerState, "Ship it", "c1");
    const [prompt] = optimistic.view.conversation;
    expect(prompt).toMatchObject({
      kind: "prose",
      role: "user",
      text: "Ship it",
      clientMessageId: "c1",
      pending: true,
    });

    const accepted = applyTurnEvent(optimistic, {
      type: "prompt_accepted",
      clientMessageId: "c1",
    });
    expect(accepted.view.conversation[0]).toMatchObject({ text: "Ship it" });
    expect(accepted.view.conversation[0]).not.toHaveProperty("pending", true);
  });

  it("keeps a prompt pending when prompt_accepted names another id", () => {
    const optimistic = applyOptimisticPrompt(initialChatSessionReducerState, "Ship it", "c1");
    const next = applyTurnEvent(optimistic, {
      type: "prompt_accepted",
      clientMessageId: "other",
    });
    expect(next.view.conversation[0]).toMatchObject({ pending: true, clientMessageId: "c1" });
  });

  it("never marks a transcript-restored prompt as pending", () => {
    const restored = applyTurnEvent(initialChatSessionReducerState, {
      type: "user_message",
      text: "From the transcript",
      itemId: "t1",
    });
    expect(restored.view.conversation[0]).not.toHaveProperty("pending");
  });
});
