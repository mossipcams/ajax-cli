import { useCallback, useEffect, useState } from "react";

export const SESSION_MODEL_STORAGE_KEY = "ajax.web.session.model";
export const DEFAULT_SESSION_MODEL = "auto";

/** Harness id the session models API expects, regardless of task-detail casing. */
export function normalizeSessionAgent(agent?: string): string {
  const trimmed = (agent ?? "cursor").trim().toLowerCase();
  return trimmed || "cursor";
}

const SESSION_MODEL_EVENT = "ajax:session-model";

export function readSessionModel(): string {
  try {
    const value = localStorage.getItem(SESSION_MODEL_STORAGE_KEY);
    if (!value || value.trim() === "") return DEFAULT_SESSION_MODEL;
    return value.trim();
  } catch {
    return DEFAULT_SESSION_MODEL;
  }
}

export function writeSessionModel(model: string): void {
  const next = model.trim() || DEFAULT_SESSION_MODEL;
  try {
    localStorage.setItem(SESSION_MODEL_STORAGE_KEY, next);
    window.dispatchEvent(new CustomEvent(SESSION_MODEL_EVENT));
  } catch {
    // Private mode / storage denied: preference just won't stick.
  }
}

export function subscribeSessionModel(listener: () => void): () => void {
  const onStorage = (event: StorageEvent) => {
    if (event.key === SESSION_MODEL_STORAGE_KEY) listener();
  };
  const onCustom = () => listener();
  window.addEventListener("storage", onStorage);
  window.addEventListener(SESSION_MODEL_EVENT, onCustom);
  return () => {
    window.removeEventListener("storage", onStorage);
    window.removeEventListener(SESSION_MODEL_EVENT, onCustom);
  };
}

export function useSessionModelPreference(): [string, (model: string) => void] {
  const [model, setModel] = useState(readSessionModel);
  useEffect(() => subscribeSessionModel(() => setModel(readSessionModel())), []);
  const setPreference = useCallback((next: string) => {
    writeSessionModel(next);
    setModel(readSessionModel());
  }, []);
  return [model, setPreference];
}

export interface SessionModelOption {
  id: string;
  label: string;
  /** Reasoning levels for this Cursor base (slim catalog from the API). */
  efforts?: string[];
  /** True when a Fast sibling exists for this Cursor base. */
  hasFast?: boolean;
}

/** A second axis beside the models, e.g. the reasoning level. */
export interface SessionModelGroup {
  /** Config id the harness answers to, e.g. `effort`. */
  id: string;
  label: string;
  options: SessionModelOption[];
  default: string;
}

/** Catalog plus the model the server launches when the request omits one. */
export interface SessionModelCatalog {
  models: SessionModelOption[];
  default: string;
  reasoning?: SessionModelGroup;
  /** Set when the harness could not be read at all (missing, or not on PATH). */
  error?: string;
}

/**
 * A selection is the model id plus any harness options, written
 * `opus|effort=high`. The server parses the same form.
 */
export function encodeModelSelection(model: string, options: Record<string, string>): string {
  const extras = Object.entries(options)
    .filter(([key, value]) => key && value)
    .map(([key, value]) => `|${key}=${value}`)
    .join("");
  return model ? `${model}${extras}` : "";
}

/** True when a host `error` event should revert an optimistic in-session model change. */
export function isSessionModelChangeFailure(message: string): boolean {
  const normalized = message.trim().toLowerCase();
  if (!normalized) return false;
  if (normalized.includes("session model")) return true;
  if (normalized.includes("could not be verified")) return true;
  if (normalized.includes("was refused")) return true;
  if (normalized.includes("unsupported model")) return true;
  if (normalized.includes("model id must not contain whitespace")) return true;
  if (normalized.includes("registry write failed")) return true;
  if (normalized.includes("cockpit state changed while updating session model")) return true;
  if (normalized.includes("agent has no acp entry point")) return true;
  if (normalized.includes("no acp mapping")) return true;
  return false;
}

export function decodeModelSelection(raw: string): {
  model: string;
  options: Record<string, string>;
} {
  const [model = "", ...rest] = raw.split("|");
  const options: Record<string, string> = {};
  for (const part of rest) {
    const [key, value] = part.split("=");
    if (key && value) options[key] = value;
  }
  return { model, options };
}

/** Matches effort suffixes on Cursor catalog ids (see core `CURSOR_EFFORT_SUFFIXES`). */
export const CURSOR_EFFORT_SUFFIXES = ["xhigh", "high", "medium", "low", "none", "max"] as const;

export interface CursorModelIntent {
  base: string;
  effort?: string;
  fast: boolean;
}

/** One collapsed Cursor model row (Fast and duplicate effort ids folded out). */
export interface CursorDisplayModel {
  base: string;
  label: string;
  efforts: readonly string[];
  hasFast: boolean;
}

function stripFastSuffix(id: string): { stem: string; fast: boolean } {
  if (id.endsWith("-fast")) {
    return { stem: id.slice(0, -5), fast: true };
  }
  return { stem: id, fast: false };
}

/** Encode a Cursor picker selection as pipe-form session_model. */
export function encodeCursorSelection(
  base: string,
  effort: string | undefined,
  fast: boolean,
  row: Pick<CursorDisplayModel, "efforts" | "hasFast">,
): string {
  if (base === DEFAULT_SESSION_MODEL || base === "auto") return base;
  const options: Record<string, string> = {};
  if (effort) options.effort = effort;
  if (row.hasFast) options.fast = fast ? "true" : "false";
  return encodeModelSelection(base, options);
}

/** Parse a Cursor ACP bracket id such as `gpt-5.6-sol[effort=high,fast=false]`. */
function parseCursorBracketId(raw: string): CursorModelIntent | null {
  const bracketStart = raw.indexOf("[");
  if (bracketStart <= 0) return null;
  const base = raw.slice(0, bracketStart);
  if (!base || !raw.endsWith("]")) return null;
  const bracket = raw.slice(bracketStart + 1, -1);
  if (!base || !bracket.includes("=")) return null;
  const intent: CursorModelIntent = { base, fast: false };
  for (const part of bracket.split(",")) {
    const eq = part.indexOf("=");
    if (eq <= 0) continue;
    const key = part.slice(0, eq).trim();
    const value = part.slice(eq + 1).trim();
    if (key === "effort" || key === "reasoning") intent.effort = value;
    else if (key === "fast") intent.fast = value === "true";
  }
  return intent;
}

/** Parse pipe-form or legacy exploded Cursor ids into picker state. */
export function decodeCursorPipeOrCatalogId(raw: string): CursorModelIntent | null {
  if (raw.includes("|")) {
    const { model, options } = decodeModelSelection(raw);
    if (!model || model === DEFAULT_SESSION_MODEL) return null;
    return {
      base: model,
      effort: options.effort ?? options.reasoning,
      fast: options.fast === "true",
    };
  }
  if (raw.includes("[")) {
    const bracket = parseCursorBracketId(raw);
    if (bracket) return bracket;
  }
  return parseCursorCatalogId(raw);
}
/** Parse a Cursor Ajax catalog id into comparable base / effort / fast pieces. */
export function parseCursorCatalogId(raw: string): CursorModelIntent | null {
  const trimmed = raw.trim();
  if (!trimmed || trimmed === DEFAULT_SESSION_MODEL) return null;

  const { stem, fast } = stripFastSuffix(trimmed);

  if (stem.startsWith("cursor-grok-")) {
    const rest = stem.slice("cursor-grok-".length);
    for (const effort of CURSOR_EFFORT_SUFFIXES) {
      const suffix = `-${effort}`;
      if (rest.endsWith(suffix)) {
        const version = rest.slice(0, rest.length - suffix.length);
        return { base: `grok-${version}`, effort, fast };
      }
    }
  }

  const lastDash = stem.lastIndexOf("-");
  if (lastDash > 0) {
    const prefix = stem.slice(0, lastDash);
    const maybeEffort = stem.slice(lastDash + 1);
    if (prefix.endsWith("-thinking") && CURSOR_EFFORT_SUFFIXES.includes(maybeEffort as (typeof CURSOR_EFFORT_SUFFIXES)[number])) {
      return {
        base: prefix.slice(0, prefix.length - "-thinking".length),
        effort: maybeEffort,
        fast,
      };
    }
  }

  for (const effort of CURSOR_EFFORT_SUFFIXES) {
    const suffix = `-${effort}`;
    if (stem.endsWith(suffix)) {
      return { base: stem.slice(0, stem.length - suffix.length), effort, fast };
    }
  }

  return { base: stem, fast };
}

function effortRank(effort: string): number {
  const index = CURSOR_EFFORT_SUFFIXES.indexOf(effort as (typeof CURSOR_EFFORT_SUFFIXES)[number]);
  return index >= 0 ? index : CURSOR_EFFORT_SUFFIXES.length;
}

function stripFastLabel(label: string): string {
  return label.replace(/\s+fast\s*$/i, "").trim();
}

function intentsMatch(a: CursorModelIntent, b: CursorModelIntent): boolean {
  return a.base === b.base && (a.effort ?? "") === (b.effort ?? "") && a.fast === b.fast;
}

/** Find the catalog id for a Cursor intent, preferring an exact catalog match. */
export function composeCursorCatalogId(
  intent: CursorModelIntent,
  catalogIds: Iterable<string>,
): string | null {
  for (const id of catalogIds) {
    const parsed = parseCursorCatalogId(id);
    if (parsed && intentsMatch(parsed, intent)) return id;
  }
  return null;
}

/** Collapse Cursor catalog rows that differ only by `-fast` or effort into one shortlist slot. */
export function collapseCursorCatalogModels(models: SessionModelOption[]): SessionModelOption[] {
  const slim = models.some(
    (option) =>
      option.id !== DEFAULT_SESSION_MODEL &&
      option.id !== "auto" &&
      (option.efforts !== undefined || option.hasFast !== undefined),
  );
  if (slim) return models;

  const auto = models.filter(
    (option) => option.id === DEFAULT_SESSION_MODEL || option.id === "auto",
  );
  const catalogIds = models.map((option) => option.id);
  const collapsed = buildCursorDisplayModels(models).map((row) => {
    const effort = row.efforts[0];
    const id =
      composeCursorCatalogId({ base: row.base, effort, fast: false }, catalogIds) ??
      models.find((option) => parseCursorCatalogId(option.id)?.base === row.base)?.id ??
      row.base;
    return { id, label: row.label };
  });
  return [...auto, ...collapsed];
}

/** Build collapsed Cursor model rows for the picker (Auto stays a normal catalog row). */
export function buildCursorDisplayModels(models: SessionModelOption[]): CursorDisplayModel[] {
  const grouped = new Map<
    string,
    { label: string; efforts: Set<string>; hasFast: boolean }
  >();

  for (const option of models) {
    if (option.id === DEFAULT_SESSION_MODEL || option.id === "auto") continue;

    if (option.efforts !== undefined || option.hasFast !== undefined) {
      grouped.set(option.id, {
        label: option.label,
        efforts: new Set(option.efforts ?? []),
        hasFast: option.hasFast ?? false,
      });
      continue;
    }

    const intent = parseCursorCatalogId(option.id);
    if (!intent) continue;
    const entry = grouped.get(intent.base) ?? {
      label: stripFastLabel(option.label),
      efforts: new Set<string>(),
      hasFast: false,
    };
    if (!intent.fast) entry.label = stripFastLabel(option.label);
    if (intent.effort) entry.efforts.add(intent.effort);
    if (intent.fast) entry.hasFast = true;
    grouped.set(intent.base, entry);
  }

  return [...grouped.entries()].map(([base, entry]) => ({
    base,
    label: entry.label,
    efforts: [...entry.efforts].sort((a, b) => effortRank(a) - effortRank(b)),
    hasFast: entry.hasFast,
  }));
}

/** Decode a persisted Cursor pipe-form or legacy catalog id into picker state. */
export function decodeCursorSelection(
  raw: string,
  displayModels: CursorDisplayModel[],
): { base: string; effort?: string; fast: boolean } | null {
  const intent = decodeCursorPipeOrCatalogId(raw);
  if (!intent) return null;
  const row = displayModels.find((model) => model.base === intent.base);
  if (!row) return null;
  const effort =
    intent.effort ??
    (row.efforts.length === 1 ? row.efforts[0] : undefined);
  return { base: intent.base, effort, fast: intent.fast };
}

/** Default effort when the operator picks a collapsed Cursor base row. */
export function defaultCursorEffort(
  row: CursorDisplayModel,
  catalogDefault?: string,
): string | undefined {
  if (row.efforts.length === 0) return undefined;
  if (row.efforts.length === 1) return row.efforts[0];
  const fromDefault = catalogDefault ? decodeCursorPipeOrCatalogId(catalogDefault) : null;
  if (fromDefault?.base === row.base && fromDefault.effort && row.efforts.includes(fromDefault.effort)) {
    return fromDefault.effort;
  }
  return row.efforts[0];
}

export function effortOptionLabel(effort: string): string {
  if (effort === "xhigh") return "Extra high";
  return effort.charAt(0).toUpperCase() + effort.slice(1);
}

/** Cursor always has Auto; a bridge harness with no answer has nothing to
 *  offer, and an empty catalog means "let the harness choose". */
function fallbackCatalog(agent: string): SessionModelCatalog {
  if (agent !== "cursor") return { models: [], default: "" };
  return {
    models: [{ id: DEFAULT_SESSION_MODEL, label: "Auto" }],
    default: DEFAULT_SESSION_MODEL,
  };
}

/** Models the given harness can run; each harness advertises its own list. */
export async function fetchSessionModels(agent = "cursor"): Promise<SessionModelCatalog> {
  const harness = normalizeSessionAgent(agent);
  const response = await fetch(`/api/session/models?agent=${encodeURIComponent(harness)}`, {
    credentials: "same-origin",
    cache: "no-store",
  });
  if (!response.ok) {
    throw new Error(`session models request failed (${response.status})`);
  }
  const body = (await response.json()) as {
    models?: Array<{
      id?: string;
      label?: string;
      efforts?: string[];
      hasFast?: boolean;
    }>;
    default?: string;
    reasoning?: SessionModelGroup;
    error?: string;
  };
  if (!Array.isArray(body.models) || body.models.length === 0) {
    return body.error
      ? { models: [], default: "", error: body.error }
      : fallbackCatalog(harness);
  }
  return {
    models: body.models
      .filter((m) => m && typeof m.id === "string" && typeof m.label === "string")
      .map((m) => ({
        id: m.id as string,
        label: m.label as string,
        ...(Array.isArray(m.efforts) ? { efforts: m.efforts.filter((e) => typeof e === "string") } : {}),
        ...(typeof m.hasFast === "boolean" ? { hasFast: m.hasFast } : {}),
      })),
    default: typeof body.default === "string" ? body.default : "",
    ...(body.reasoning && Array.isArray(body.reasoning.options)
      ? { reasoning: body.reasoning }
      : {}),
  };
}
