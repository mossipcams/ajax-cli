import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  DEFAULT_SESSION_MODEL,
  SESSION_MODEL_STORAGE_KEY,
  fetchSessionModels,
  isSessionModelChangeFailure,
  normalizeSessionAgent,
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
    expect(isSessionModelChangeFailure("session model composer-2.5 was refused — model refused")).toBe(
      true,
    );
    expect(isSessionModelChangeFailure("session model composer-2.5 could not be verified — harness did not report an applied model")).toBe(
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

  it("asks for the normalized harness and rejects when the API cannot answer", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 503 });
    vi.stubGlobal("fetch", fetchMock);

    await expect(fetchSessionModels("cursor")).rejects.toThrow("session models request failed");
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain("agent=cursor");

    await expect(fetchSessionModels("Codex")).rejects.toThrow("session models request failed");
    expect(String(fetchMock.mock.calls[1]?.[0])).toContain("agent=codex");
  });

  it("normalizes harness ids for catalog lookup", () => {
    expect(normalizeSessionAgent("Cursor")).toBe("cursor");
    expect(normalizeSessionAgent("  ")).toBe("cursor");
  });

  it("offers Cursor Auto when the harness answers with an empty list", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ models: [], default: "" }),
      }),
    );

    await expect(fetchSessionModels("cursor")).resolves.toEqual({
      models: [{ id: DEFAULT_SESSION_MODEL, label: "Auto" }],
      default: DEFAULT_SESSION_MODEL,
    });

    await expect(fetchSessionModels("codex")).resolves.toEqual({ models: [], default: "" });
  });
});
