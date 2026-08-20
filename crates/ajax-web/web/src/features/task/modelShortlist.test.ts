import { describe, it, expect } from "vitest";
import { buildModelShortlist, SHORTLIST_CAP } from "./modelShortlist";

const cursorCatalog = [
  { id: "auto", label: "Auto" },
  { id: "composer-2.5", label: "Composer 2.5" },
  { id: "composer-2.5-fast", label: "Composer 2.5 Fast" },
  { id: "cursor-grok-4.6-high", label: "Grok 4.6" },
  { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 Fast" },
  { id: "gpt-5.6-sol-medium", label: "GPT 5.6" },
  { id: "gpt-5.6-sol-high", label: "GPT 5.6 High" },
  { id: "claude-opus-4.6", label: "Opus 4.6" },
  { id: "claude-sonnet-4.6", label: "Sonnet 4.6" },
  { id: "gemini-3.7-flash", label: "Gemini Flash" },
  ...Array.from({ length: 8 }, (_, index) => ({
    id: `extra-${index}`,
    label: `Extra ${index}`,
  })),
];

describe("buildModelShortlist", () => {
  it("caps Cursor catalogs at about ten ranked models", () => {
    const { shortlist, hasMore } = buildModelShortlist(cursorCatalog, "cursor", {
      catalogDefault: "cursor-grok-4.6-high",
    });
    expect(shortlist.length).toBeLessThanOrEqual(SHORTLIST_CAP + 2);
    expect(hasMore).toBe(true);
    expect(shortlist.map((option) => option.id)).toEqual(
      expect.arrayContaining(["auto", "composer-2.5", "cursor-grok-4.6-high"]),
    );
  });

  it("pins the current selection even when it is outside the popular rank", () => {
    const { shortlist } = buildModelShortlist(cursorCatalog, "cursor", {
      currentModelId: "extra-7",
      catalogDefault: "auto",
    });
    expect(shortlist.some((option) => option.id === "extra-7")).toBe(true);
  });

  it("uses advertised order for non-Cursor harnesses", () => {
    const codex = Array.from({ length: 14 }, (_, index) => ({
      id: `model-${index}`,
      label: `Model ${index}`,
    }));
    const { shortlist, hasMore } = buildModelShortlist(codex, "codex", {
      catalogDefault: "model-0",
    });
    expect(shortlist).toHaveLength(SHORTLIST_CAP);
    expect(shortlist[0].id).toBe("model-0");
    expect(hasMore).toBe(true);
  });

  it("does not fail when a ranked id is missing from the live catalog", () => {
    const sparse = [{ id: "auto", label: "Auto" }, { id: "only-one", label: "Only" }];
    const { shortlist, hasMore } = buildModelShortlist(sparse, "cursor", {});
    expect(shortlist.map((option) => option.id)).toEqual(["auto", "only-one"]);
    expect(hasMore).toBe(false);
  });

  it("keeps exploded Cursor variants as separate shortlist slots", () => {
    const exploded = [
      { id: "auto", label: "Auto" },
      { id: "composer-2.5", label: "Composer 2.5" },
      { id: "composer-2.5-fast", label: "Composer 2.5 Fast" },
      { id: "cursor-grok-4.6-high", label: "Grok 4.6" },
      { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 Fast" },
    ];
    const { shortlist } = buildModelShortlist(exploded, "cursor", {
      catalogDefault: "cursor-grok-4.6-high",
    });
    const ids = shortlist.map((option) => option.id);
    expect(ids).toContain("composer-2.5");
    expect(ids).toContain("composer-2.5-fast");
    expect(ids).toContain("cursor-grok-4.6-high");
    expect(ids).toContain("cursor-grok-4.6-high-fast");
  });

  it("caps generic Cursor catalogs without leaking model-11 (#948)", () => {
    const generic = Array.from({ length: 12 }, (_, index) => ({
      id: `model-${index}`,
      label: `Model ${index}`,
    }));
    const { shortlist, hasMore } = buildModelShortlist(generic, "cursor", {
      currentModelId: "model-3",
      catalogDefault: "model-0",
    });
    expect(shortlist.map((option) => option.id)).not.toContain("model-11");
    expect(hasMore).toBe(true);
  });
});
