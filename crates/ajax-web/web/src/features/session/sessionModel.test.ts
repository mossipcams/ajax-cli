import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  DEFAULT_SESSION_MODEL,
  SESSION_MODEL_STORAGE_KEY,
  fetchSessionModels,
  isSessionModelChangeFailure,
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

  it("recognizes host errors that should revert an in-session model change (#942)", () => {
    expect(isSessionModelChangeFailure("session model change needs a task Ajax started over ACP")).toBe(
      true,
    );
    expect(isSessionModelChangeFailure("unsupported model")).toBe(true);
    expect(isSessionModelChangeFailure("ACP process exited")).toBe(false);
    expect(isSessionModelChangeFailure("queued prompt failed: prompt already in flight")).toBe(false);
  });

  it("persists the chosen model", () => {
    writeSessionModel("composer-2.5");
    expect(localStorage.getItem(SESSION_MODEL_STORAGE_KEY)).toBe("composer-2.5");
    expect(readSessionModel()).toBe("composer-2.5");
  });

  it("fetches models and the launch default from the session API", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "auto", label: "Auto" },
            { id: "composer-2.5", label: "Composer 2.5" },
          ],
          default: "cursor-grok-4.6-high",
        }),
      }),
    );
    await expect(fetchSessionModels()).resolves.toEqual({
      models: [
        { id: "auto", label: "Auto" },
        { id: "composer-2.5", label: "Composer 2.5" },
      ],
      default: "cursor-grok-4.6-high",
    });
  });

  it("reports no default when the API omits one", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ models: [{ id: "composer-2.5", label: "Composer 2.5" }] }),
      }),
    );
    await expect(fetchSessionModels()).resolves.toEqual({
      models: [{ id: "composer-2.5", label: "Composer 2.5" }],
      default: "",
    });
  });

  it("asks for the requested harness and offers Cursor Auto when it cannot answer", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: false });
    vi.stubGlobal("fetch", fetchMock);

    await expect(fetchSessionModels("cursor")).resolves.toEqual({
      models: [{ id: DEFAULT_SESSION_MODEL, label: "Auto" }],
      default: DEFAULT_SESSION_MODEL,
    });
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain("agent=cursor");

    // A bridge harness has no Auto sentinel: an empty catalog means "harness picks".
    await expect(fetchSessionModels("codex")).resolves.toEqual({ models: [], default: "" });
  });
});
