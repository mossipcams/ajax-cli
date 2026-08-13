import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  DEFAULT_SESSION_MODEL,
  SESSION_MODEL_STORAGE_KEY,
  fetchSessionModels,
  readSessionModel,
  writeSessionModel,
} from "./sessionModel";

describe("sessionModel", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it("defaults to auto", () => {
    expect(readSessionModel()).toBe(DEFAULT_SESSION_MODEL);
  });

  it("persists the chosen model", () => {
    writeSessionModel("composer-2.5");
    expect(localStorage.getItem(SESSION_MODEL_STORAGE_KEY)).toBe("composer-2.5");
    expect(readSessionModel()).toBe("composer-2.5");
  });

  it("fetches models from the session API", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "auto", label: "Auto" },
            { id: "composer-2.5", label: "Composer 2.5" },
          ],
        }),
      }),
    );
    await expect(fetchSessionModels()).resolves.toEqual([
      { id: "auto", label: "Auto" },
      { id: "composer-2.5", label: "Composer 2.5" },
    ]);
  });
});
