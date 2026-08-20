import {
  collapseCursorCatalogModels,
  decodeCursorPipeOrCatalogId,
  DEFAULT_SESSION_MODEL,
  normalizeSessionAgent,
  parseCursorCatalogId,
  type SessionModelOption,
} from "./desiredModel";

export const SHORTLIST_CAP = 10;

type Matcher = (id: string, label: string) => boolean;

/** Cursor popular-model rank: first catalog match wins each slot. */
const CURSOR_RANK: Matcher[] = [
  (id) => id === DEFAULT_SESSION_MODEL || id === "auto",
  (id) => id.includes("composer-2.5"),
  (id) => id.includes("composer"),
  (id) => id.includes("grok-4.6") || id.includes("cursor-grok-4.6"),
  (id) => id.includes("cursor-grok") || id.includes("grok"),
  (id) => id.includes("gpt-5.6") || id.includes("gpt-5-6"),
  (id) => id.includes("gpt-5"),
  (id) => id.includes("gpt"),
  (id, label) => /opus/.test(id) || /opus/.test(label),
  (id, label) => /sonnet/.test(id) || /sonnet/.test(label),
  (id, label) => /gemini/.test(id) || /gemini/.test(label),
];

function norm(option: SessionModelOption): { id: string; label: string } {
  return { id: option.id.toLowerCase(), label: option.label.toLowerCase() };
}

function pickRanked(models: SessionModelOption[], agent: string): SessionModelOption[] {
  if (normalizeSessionAgent(agent) !== "cursor") {
    return models.slice(0, SHORTLIST_CAP);
  }

  const picked: SessionModelOption[] = [];
  const used = new Set<string>();

  const trySlot = (matcher: Matcher) => {
    for (const option of models) {
      if (used.has(option.id)) continue;
      const { id, label } = norm(option);
      if (matcher(id, label)) {
        picked.push(option);
        used.add(option.id);
        return;
      }
    }
  };

  for (const matcher of CURSOR_RANK) {
    if (picked.length >= SHORTLIST_CAP) break;
    trySlot(matcher);
  }

  for (const option of models) {
    if (picked.length >= SHORTLIST_CAP) break;
    if (used.has(option.id)) continue;
    picked.push(option);
    used.add(option.id);
  }

  return picked;
}

function findById(models: SessionModelOption[], id: string | undefined): SessionModelOption | undefined {
  if (!id) return undefined;
  return models.find((option) => option.id === id);
}

/** ~10 popular models plus pinned Auto, harness default, and current selection. */
export function buildModelShortlist(
  models: SessionModelOption[],
  agent: string,
  pins: { currentModelId?: string; catalogDefault?: string },
): { shortlist: SessionModelOption[]; hasMore: boolean } {
  const catalog =
    normalizeSessionAgent(agent) === "cursor" ? collapseCursorCatalogModels(models) : models;
  const ranked = pickRanked(catalog, agent);
  const shortlist: SessionModelOption[] = [];
  const seen = new Set<string>();

  const add = (option: SessionModelOption | undefined) => {
    if (!option || seen.has(option.id)) return;
    shortlist.push(option);
    seen.add(option.id);
  };

  add(findById(catalog, DEFAULT_SESSION_MODEL));
  add(findById(catalog, "auto"));
  add(findById(catalog, pins.catalogDefault));
  if (pins.currentModelId) {
    if (normalizeSessionAgent(agent) === "cursor") {
      const currentIntent =
        decodeCursorPipeOrCatalogId(pins.currentModelId) ??
        parseCursorCatalogId(pins.currentModelId);
      const pinned = currentIntent
        ? catalog.find((option) => option.id === currentIntent.base)
        : findById(catalog, pins.currentModelId);
      add(pinned);
    } else {
      add(findById(catalog, pins.currentModelId));
    }
  }

  for (const option of ranked) add(option);

  return { shortlist, hasMore: catalog.length > shortlist.length };
}
