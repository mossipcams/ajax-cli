import { describe, it, expect, afterEach, vi } from "vitest";
import {
  TASK_TERMINAL_PREFERENCE_STORAGE_KEY,
  readTaskTerminalPreferred,
  writeTaskTerminalPreferred,
  clearTaskTerminalPreferred,
  subscribeTaskViewPreference,
} from "./taskViewPreference";

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("taskViewPreference", () => {
  it("defaults terminal preference to false for unknown handles", () => {
    expect(readTaskTerminalPreferred("web/fix-login")).toBe(false);
  });

  it("persists terminal preference per handle in localStorage", () => {
    writeTaskTerminalPreferred("web/fix-login");
    expect(localStorage.getItem(TASK_TERMINAL_PREFERENCE_STORAGE_KEY)).toBe(
      JSON.stringify(["web/fix-login"]),
    );
    expect(readTaskTerminalPreferred("web/fix-login")).toBe(true);
    expect(readTaskTerminalPreferred("web/other")).toBe(false);
  });

  it("clears terminal preference for one handle without affecting others", () => {
    writeTaskTerminalPreferred("web/a");
    writeTaskTerminalPreferred("web/b");
    clearTaskTerminalPreferred("web/a");
    expect(readTaskTerminalPreferred("web/a")).toBe(false);
    expect(readTaskTerminalPreferred("web/b")).toBe(true);
  });

  it("notifies subscribers when preference changes", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeTaskViewPreference(listener);
    writeTaskTerminalPreferred("web/fix-login");
    expect(listener).toHaveBeenCalledOnce();
    unsubscribe();
    clearTaskTerminalPreferred("web/fix-login");
    expect(listener).toHaveBeenCalledOnce();
  });
});
