import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  DEFAULT_SESSION_MODEL,
  SESSION_MODEL_STORAGE_KEY,
  buildCursorDisplayModels,
  composeCursorCatalogId,
  encodeCursorSelection,
  decodeCursorPipeOrCatalogId,
  fetchSessionModels,
  mergeCursorEffortChoices,
  normalizeSessionAgent,
  parseCursorCatalogId,
  readSessionModel,
  writeSessionModel,
} from "./desiredModel";

describe("desiredModel", () => {
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

  it("parses and composes Cursor catalog ids with effort and Fast (#979)", () => {
    const catalog = [
      "auto",
      "composer-2.5",
      "composer-2.5-fast",
      "cursor-grok-4.6-high",
      "cursor-grok-4.6-high-fast",
      "gpt-5.6-sol-medium",
      "gpt-5.6-sol-high",
    ];

    expect(parseCursorCatalogId("cursor-grok-4.6-high")).toEqual({
      base: "grok-4.6",
      effort: "high",
      fast: false,
    });
    expect(parseCursorCatalogId("composer-2.5-fast")).toEqual({
      base: "composer-2.5",
      fast: true,
    });
    expect(
      composeCursorCatalogId(
        { base: "grok-4.6", effort: "high", fast: true },
        catalog,
      ),
    ).toBe("cursor-grok-4.6-high-fast");
    expect(
      composeCursorCatalogId(
        { base: "composer-2.5", fast: false },
        catalog,
      ),
    ).toBe("composer-2.5");
    expect(
      composeCursorCatalogId(
        { base: "gpt-5.6-sol", effort: "medium", fast: false },
        catalog,
      ),
    ).toBe("gpt-5.6-sol-medium");
  });

  it("decodes Cursor ACP bracket-form snapshot ids (#989)", () => {
    expect(decodeCursorPipeOrCatalogId("gpt-5.6-sol[fast=false]")).toEqual({
      base: "gpt-5.6-sol",
      fast: false,
    });
    expect(decodeCursorPipeOrCatalogId("gpt-5.6-sol[effort=high,fast=false]")).toEqual({
      base: "gpt-5.6-sol",
      effort: "high",
      fast: false,
    });
    expect(decodeCursorPipeOrCatalogId("gpt-5.6-sol[reasoning=high,fast=false]")).toEqual({
      base: "gpt-5.6-sol",
      effort: "high",
      fast: false,
    });
  });

  it("encodes and decodes pipe-form Cursor session_model (#979)", () => {
    expect(
      encodeCursorSelection("grok-4.6", "high", false, { efforts: ["high"], hasFast: true }),
    ).toBe("grok-4.6|effort=high|fast=false");
    expect(
      encodeCursorSelection("grok-4.6", "high", true, { efforts: ["high"], hasFast: true }),
    ).toBe("grok-4.6|effort=high|fast=true");
    expect(
      encodeCursorSelection("composer-2.5", undefined, false, { efforts: [], hasFast: true }),
    ).toBe("composer-2.5|fast=false");
    expect(decodeCursorPipeOrCatalogId("grok-4.6|effort=high|fast=false")).toEqual({
      base: "grok-4.6",
      effort: "high",
      fast: false,
    });
    expect(decodeCursorPipeOrCatalogId("cursor-grok-4.6-high")).toEqual({
      base: "grok-4.6",
      effort: "high",
      fast: false,
    });
  });

  it("keeps thinking in the Cursor catalog base (#1004)", () => {
    expect(parseCursorCatalogId("claude-opus-5-thinking-high")).toEqual({
      base: "claude-opus-5-thinking",
      effort: "high",
      fast: false,
    });
    expect(parseCursorCatalogId("claude-opus-5-high")).toEqual({
      base: "claude-opus-5",
      effort: "high",
      fast: false,
    });

    const display = buildCursorDisplayModels([
      { id: "claude-opus-5-high", label: "Claude Opus 5 High" },
      { id: "claude-opus-5-thinking-high", label: "Claude Opus 5 Thinking High" },
    ]);
    expect(display.map((row) => row.base)).toEqual([
      "claude-opus-5",
      "claude-opus-5-thinking",
    ]);
  });

  it("merges catalog Grok efforts with sparse live thought_level choices (#1004)", () => {
    expect(
      mergeCursorEffortChoices(["low", "medium", "high", "xhigh"], [{ value: "high", name: "High" }]),
    ).toEqual([
      { value: "xhigh", label: "Extra high" },
      { value: "high", label: "High" },
      { value: "medium", label: "Medium" },
      { value: "low", label: "Low" },
    ]);
    expect(mergeCursorEffortChoices(["high"], [{ value: "high", name: "High" }])).toEqual([]);
  });

  it("passes through ACP-aligned Cursor catalog labels from the session API", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "auto", label: "Auto" },
            { id: "grok-4.6", label: "Grok 4.6", efforts: ["high"], hasFast: true },
            { id: "gpt-5.6-sol", label: "GPT-5.6-Sol", efforts: ["high"], hasFast: true },
          ],
          default: "grok-4.6",
        }),
      }),
    );
    await expect(fetchSessionModels("cursor")).resolves.toEqual({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["high"], hasFast: true },
        { id: "gpt-5.6-sol", label: "GPT-5.6-Sol", efforts: ["high"], hasFast: true },
      ],
      default: "grok-4.6",
    });
  });
});
