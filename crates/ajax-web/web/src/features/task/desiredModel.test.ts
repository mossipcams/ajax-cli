import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  DEFAULT_SESSION_MODEL,
  SESSION_MODEL_STORAGE_KEY,
  buildCursorDisplayModels,
  catalogIdToStoragePipe,
  composeCursorCatalogId,
  encodeCursorSelection,
  defaultCatalogIdForCursorGroup,
  decodeCursorPipeOrCatalogId,
  fetchSessionModels,
  groupCursorCatalogModels,
  mergeCursorEffortChoices,
  normalizeSessionAgent,
  parseCursorCatalogId,
  readSessionModel,
  resolveCursorCatalogSelection,
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

  it("maps exploded catalog ids to pipe storage", () => {
    const catalog = [
      "cursor-grok-4.6-high",
      "cursor-grok-4.6-high-fast",
      "claude-opus-5-thinking-high",
    ];
    expect(catalogIdToStoragePipe("cursor-grok-4.6-high", catalog)).toBe(
      "grok-4.6|effort=high|fast=false",
    );
    expect(catalogIdToStoragePipe("composer-2.5-fast", catalog)).toBe("composer-2.5|fast=true");
    expect(catalogIdToStoragePipe("claude-opus-5-thinking-high", catalog)).toBe(
      "claude-opus-5|thinking=true|effort=high",
    );
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

  it("groups exploded Cursor catalog ids under family headers", () => {
    const { auto, groups } = groupCursorCatalogModels([
      { id: "auto", label: "Auto" },
      { id: "composer-2.5", label: "Composer 2.5" },
      { id: "composer-2.5-fast", label: "Composer 2.5 Fast" },
      { id: "cursor-grok-4.6-high", label: "Grok 4.6" },
      { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 Fast" },
    ]);
    expect(auto.map((option) => option.id)).toEqual(["auto"]);
    expect(groups).toEqual([
      {
        base: "composer-2.5",
        label: "Composer 2.5",
        variants: [
          { id: "composer-2.5", label: "Composer 2.5" },
          { id: "composer-2.5-fast", label: "Composer 2.5 Fast" },
        ],
      },
      {
        base: "grok-4.6",
        label: "Grok 4.6",
        variants: [
          { id: "cursor-grok-4.6-high", label: "Grok 4.6 High" },
          { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 Fast" },
        ],
      },
    ]);
  });

  it("keeps the family name on every variant under the group header", () => {
    const { groups } = groupCursorCatalogModels([
      { id: "cursor-grok-4.5-high", label: "Grok 4.5" },
      { id: "cursor-grok-4.5-high-fast", label: "Grok 4.5 Fast" },
    ]);
    expect(groups).toEqual([
      {
        base: "grok-4.5",
        label: "Grok 4.5",
        variants: [
          { id: "cursor-grok-4.5-high", label: "Grok 4.5 High" },
          { id: "cursor-grok-4.5-high-fast", label: "Grok 4.5 Fast" },
        ],
      },
    ]);
  });

  it("strips effort suffixes from Cursor family headers when labels include them", () => {
    const { groups } = groupCursorCatalogModels([
      { id: "cursor-grok-4.6-xhigh", label: "Grok 4.6 Extra High" },
      { id: "cursor-grok-4.6-high", label: "Grok 4.6 High" },
      { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 Extra High Fast" },
    ]);
    expect(groups).toEqual([
      {
        base: "grok-4.6",
        label: "Grok 4.6",
        variants: [
          { id: "cursor-grok-4.6-xhigh", label: "Grok 4.6 Extra High" },
          { id: "cursor-grok-4.6-high", label: "Grok 4.6 High" },
          { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 Extra High Fast" },
        ],
      },
    ]);
  });

  it("decodes thinking from pipe-form and bracket ids (#1013)", () => {
    expect(decodeCursorPipeOrCatalogId("claude-opus-5|thinking=true|effort=high")).toEqual({
      base: "claude-opus-5",
      thinking: true,
      effort: "high",
      fast: false,
    });
    expect(
      decodeCursorPipeOrCatalogId(
        "claude-opus-5[thinking=true,context=200k,effort=high,fast=false]",
      ),
    ).toEqual({
      base: "claude-opus-5",
      thinking: true,
      effort: "high",
      fast: false,
    });
  });

  it("resolves thinking pipe storage to the thinking catalog variant (#1013)", () => {
    const ids = ["claude-opus-5-high", "claude-opus-5-thinking-high"] as const;
    expect(resolveCursorCatalogSelection("claude-opus-5|thinking=true|effort=high", ids)).toBe(
      "claude-opus-5-thinking-high",
    );
    expect(resolveCursorCatalogSelection("claude-opus-5|effort=high", ids)).toBe(
      "claude-opus-5-high",
    );
  });

  it("resolves legacy pipe-form values to exploded Cursor catalog ids", () => {
    const ids = [
      "auto",
      "cursor-grok-4.6-high",
      "cursor-grok-4.6-high-fast",
    ] as const;
    expect(resolveCursorCatalogSelection("cursor-grok-4.6-high", ids)).toBe(
      "cursor-grok-4.6-high",
    );
    expect(resolveCursorCatalogSelection("grok-4.6|effort=high|fast=true", ids)).toBe(
      "cursor-grok-4.6-high-fast",
    );
    expect(
      resolveCursorCatalogSelection("grok-4.6|reasoning=xhigh|fast=false", [
        "cursor-grok-4.6-xhigh",
        "cursor-grok-4.6-high",
      ]),
    ).toBe("cursor-grok-4.6-xhigh");
  });

  it("picks the non-fast preferred effort when tapping a Cursor family header", () => {
    const { groups } = groupCursorCatalogModels([
      { id: "cursor-grok-4.6-xhigh", label: "Grok 4.6 Extra High" },
      { id: "cursor-grok-4.6-high", label: "Grok 4.6 High" },
      { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 High Fast" },
    ]);
    const group = groups[0]!;
    const ids = group.variants.map((variant) => variant.id);
    expect(defaultCatalogIdForCursorGroup(group, "cursor-grok-4.6-high", ids)).toBe(
      "cursor-grok-4.6-high",
    );
  });
});
