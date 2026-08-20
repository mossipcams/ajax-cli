import { describe, it, expect, afterEach, vi } from "vitest";
import {
  ORCHESTRATION_CHAT_STORAGE_KEY,
  readOrchestrationChatEnabled,
  writeOrchestrationChatEnabled,
  subscribeOrchestrationChat,
} from "./orchestrationChatPreference";

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("orchestrationChatPreference", () => {
  it("defaults orchestration chat to true when the key is missing", () => {
    expect(readOrchestrationChatEnabled()).toBe(true);
  });

  it("disables orchestration chat only when explicitly set to false", () => {
    localStorage.setItem(ORCHESTRATION_CHAT_STORAGE_KEY, "false");
    expect(readOrchestrationChatEnabled()).toBe(false);
  });

  it("persists enabled preference in localStorage", () => {
    writeOrchestrationChatEnabled(true);
    expect(localStorage.getItem(ORCHESTRATION_CHAT_STORAGE_KEY)).toBe("true");
    expect(readOrchestrationChatEnabled()).toBe(true);
    writeOrchestrationChatEnabled(false);
    expect(readOrchestrationChatEnabled()).toBe(false);
  });

  it("notifies subscribers when preference changes", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeOrchestrationChat(listener);
    writeOrchestrationChatEnabled(true);
    expect(listener).toHaveBeenCalledOnce();
    unsubscribe();
    writeOrchestrationChatEnabled(false);
    expect(listener).toHaveBeenCalledOnce();
  });
});
