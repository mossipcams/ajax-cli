import { useCallback, useEffect, useState } from "react";

import {
  DEFAULT_SESSION_MODEL,
  decodeModelSelection,
  encodeModelSelection,
} from "@/shared/lib/sessionModelSelection";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import { modelLiveOption, readLiveSelectCurrent } from "@/shared/lib/liveSessionConfig";

export { DEFAULT_SESSION_MODEL, decodeModelSelection, encodeModelSelection };

export const SESSION_MODEL_STORAGE_KEY = "ajax.web.session.model";

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

/** Last-advertised harness configOptions for New Task pickers (AoE contract). */
export interface SessionOptionCatalog {
  agent: string;
  configOptions: LiveSessionConfigOption[];
  error?: string;
}

/** Models the given harness advertises on its option catalog. */
export async function fetchSessionOptionCatalog(agent = "cursor"): Promise<SessionOptionCatalog> {
  const harness = normalizeSessionAgent(agent);
  const response = await fetch(
    `/api/session/option-catalog?agent=${encodeURIComponent(harness)}`,
    { credentials: "same-origin", cache: "no-store" },
  );
  if (!response.ok) {
    throw new Error(`session option catalog request failed (${response.status})`);
  }
  const body = (await response.json()) as {
    agent?: string;
    configOptions?: LiveSessionConfigOption[];
    error?: string;
  };
  const configOptions = Array.isArray(body.configOptions) ? body.configOptions : [];
  if (body.error && configOptions.length === 0) {
    return { agent: harness, configOptions: [], error: body.error };
  }
  return {
    agent: typeof body.agent === "string" ? body.agent : harness,
    configOptions,
    ...(body.error ? { error: body.error } : {}),
  };
}

export function modelChoicesFromOptionCatalog(
  catalog: SessionOptionCatalog,
): { models: SessionModelOption[]; default: string } {
  const model = modelLiveOption(catalog.configOptions);
  if (!model?.choices.length) {
    return { models: [], default: "" };
  }
  const models = model.choices.map((choice) => ({
    id: choice.value,
    label: choice.name,
  }));
  const current = readLiveSelectCurrent(model);
  return { models, default: current ?? models[0]?.id ?? "" };
}

/** Matches effort suffixes on Cursor catalog ids (see core `CURSOR_EFFORT_SUFFIXES`). */
export const CURSOR_EFFORT_SUFFIXES = ["xhigh", "high", "medium", "low", "none", "max"] as const;

export interface CursorModelIntent {
  base: string;
  /** Explicit pipe/bracket axis; catalog ids encode thinking via `-thinking` in `base`. */
  thinking?: boolean;
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
  thinking = false,
): string {
  if (base === DEFAULT_SESSION_MODEL || base === "auto") return base;
  const options: Record<string, string> = {};
  if (thinking) options.thinking = "true";
  if (effort) options.effort = effort;
  if (row.hasFast) options.fast = fast ? "true" : "false";
  return encodeModelSelection(base, options);
}

/** Map an exploded Cursor catalog id to Ajax pipe storage. */
export function catalogIdToStoragePipe(
  catalogId: string,
  catalogIds: readonly string[] = [],
): string {
  const trimmed = catalogId.trim();
  if (!trimmed || trimmed === DEFAULT_SESSION_MODEL || trimmed === "auto") {
    return trimmed || DEFAULT_SESSION_MODEL;
  }
  const intent = parseCursorCatalogId(trimmed);
  if (!intent) return trimmed;
  const thinking = intent.base.endsWith("-thinking");
  const base = thinking ? intent.base.slice(0, -"-thinking".length) : intent.base;
  const hasFast =
    intent.fast ||
    catalogIds.some((id) => {
      const parsed = parseCursorCatalogId(id);
      return parsed?.base === intent.base && parsed.fast;
    });
  return encodeCursorSelection(
    base,
    intent.effort,
    intent.fast,
    { efforts: intent.effort ? [intent.effort] : [], hasFast },
    thinking,
  );
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
    else if (key === "thinking") intent.thinking = value === "true";
  }
  return intent;
}

/** Parse pipe-form or legacy exploded Cursor ids into picker state. */
export function decodeCursorPipeOrCatalogId(raw: string): CursorModelIntent | null {
  if (raw.includes("|")) {
    const { model, options } = decodeModelSelection(raw);
    if (!model || model === DEFAULT_SESSION_MODEL) return null;
    const thinking =
      options.thinking === "true"
        ? true
        : options.thinking === "false"
          ? false
          : undefined;
    return {
      base: model,
      ...(thinking !== undefined ? { thinking } : {}),
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
        base: prefix,
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

/** Strip effort words agent-models embed in family labels (Extra High before High). */
function stripEffortFromLabel(label: string): string {
  let result = stripFastLabel(label);
  const suffixes = [
    /\s+extra\s+high$/i,
    /\s+xhigh$/i,
    /\s+high$/i,
    /\s+medium$/i,
    /\s+low$/i,
    /\s+max$/i,
    /\s+none$/i,
  ];
  for (const pattern of suffixes) {
    if (pattern.test(result)) {
      result = result.replace(pattern, "").trim();
      break;
    }
  }
  return result;
}

function cursorFamilyStem(base: string): string {
  return base.endsWith("-thinking") ? base.slice(0, -"-thinking".length) : base;
}

function canonicalThinking(intent: CursorModelIntent): boolean {
  if (intent.thinking !== undefined) return intent.thinking;
  return intent.base.endsWith("-thinking");
}

function intentsMatch(a: CursorModelIntent, b: CursorModelIntent): boolean {
  if (canonicalThinking(a) !== canonicalThinking(b)) return false;
  if (cursorFamilyStem(a.base) !== cursorFamilyStem(b.base)) return false;
  if ((a.effort ?? "") !== (b.effort ?? "")) return false;
  return a.fast === b.fast;
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
        label: stripEffortFromLabel(option.label),
        efforts: new Set(option.efforts ?? []),
        hasFast: option.hasFast ?? false,
      });
      continue;
    }

    const intent = parseCursorCatalogId(option.id);
    if (!intent) continue;
    const entry = grouped.get(intent.base) ?? {
      label: stripEffortFromLabel(option.label),
      efforts: new Set<string>(),
      hasFast: false,
    };
    if (!intent.fast) entry.label = stripEffortFromLabel(option.label);
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

export interface CursorCatalogVariant {
  id: string;
  label: string;
}

export interface CursorCatalogGroup {
  base: string;
  label: string;
  variants: CursorCatalogVariant[];
}

function humanizeCursorVariant(intent: CursorModelIntent | null): string {
  if (!intent) return "";
  const parts: string[] = [];
  if (intent.effort) parts.push(effortOptionLabel(intent.effort));
  if (intent.fast) parts.push("Fast");
  return parts.join(" ");
}

/** Catalog sometimes sends just "Fast" / "High"; those belong under the family name. */
function isBareVariantLabel(label: string): boolean {
  return /^(extra\s+high|high|medium|low|max|none|xhigh)(\s+fast)?$|^fast$/i.test(
    label.trim(),
  );
}

function variantLabelForCursorModel(option: SessionModelOption, groupLabel: string): string {
  const catalogLabel = option.label.trim();
  const intent = parseCursorCatalogId(option.id);
  const suffix = humanizeCursorVariant(intent);
  const composed = suffix ? `${groupLabel} ${suffix}` : "";
  if (catalogLabel && catalogLabel !== groupLabel && !isBareVariantLabel(catalogLabel)) {
    return catalogLabel;
  }
  if (composed) return composed;
  return catalogLabel || option.id;
}

function sortCursorVariants(variants: CursorCatalogVariant[]): CursorCatalogVariant[] {
  return [...variants].sort((a, b) => {
    const left = parseCursorCatalogId(a.id);
    const right = parseCursorCatalogId(b.id);
    const effortDiff =
      effortRank(left?.effort ?? "") - effortRank(right?.effort ?? "");
    if (effortDiff !== 0) return effortDiff;
    return Number(left?.fast ?? false) - Number(right?.fast ?? false);
  });
}

/** Group exploded Cursor catalog ids under family headers for New Task / idle Switch. */
export function groupCursorCatalogModels(models: SessionModelOption[]): {
  auto: SessionModelOption[];
  groups: CursorCatalogGroup[];
} {
  const auto = models.filter(
    (option) => option.id === DEFAULT_SESSION_MODEL || option.id === "auto",
  );
  const displayRows = buildCursorDisplayModels(models);
  const grouped = new Map<string, CursorCatalogVariant[]>();
  const groupLabels = new Map(displayRows.map((row) => [row.base, row.label]));

  for (const option of models) {
    if (option.id === DEFAULT_SESSION_MODEL || option.id === "auto") continue;
    const intent = parseCursorCatalogId(option.id);
    const base = intent?.base ?? option.id;
    if (!grouped.has(base)) grouped.set(base, []);
    grouped.get(base)!.push({
      id: option.id,
      label: variantLabelForCursorModel(option, groupLabels.get(base) ?? option.label),
    });
  }

  const groups = displayRows.map((row) => ({
    base: row.base,
    label: row.label,
    variants: sortCursorVariants(grouped.get(row.base) ?? []),
  }));

  return { auto, groups };
}

/** Default exploded catalog id when the operator taps a Cursor family header. */
export function defaultCatalogIdForCursorGroup(
  group: CursorCatalogGroup,
  catalogDefault: string,
  catalogIds: readonly string[],
): string {
  const efforts = new Set<string>();
  let hasFast = false;
  for (const variant of group.variants) {
    const intent = parseCursorCatalogId(variant.id);
    if (!intent) continue;
    if (intent.effort) efforts.add(intent.effort);
    if (intent.fast) hasFast = true;
  }
  const row: CursorDisplayModel = {
    base: group.base,
    label: group.label,
    efforts: [...efforts].sort((a, b) => effortRank(a) - effortRank(b)),
    hasFast,
  };
  const effort = defaultCursorEffort(row, catalogDefault);
  return (
    composeCursorCatalogId({ base: group.base, effort, fast: false }, catalogIds) ??
    group.variants.find((variant) => !parseCursorCatalogId(variant.id)?.fast)?.id ??
    group.variants[0]?.id ??
    group.base
  );
}

/** Resolve a stored value to an exploded Cursor catalog id when possible. */
export function resolveCursorCatalogSelection(
  value: string,
  catalogIds: readonly string[],
): string {
  const trimmed = value.trim();
  if (!trimmed) return "";
  if (catalogIds.includes(trimmed)) return trimmed;
  const intent = decodeCursorPipeOrCatalogId(trimmed);
  if (intent) {
    const composed = composeCursorCatalogId(intent, catalogIds);
    if (composed) return composed;
  }
  const { model } = decodeModelSelection(trimmed);
  if (model && catalogIds.includes(model)) return model;
  return trimmed;
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

export interface CursorEffortChoice {
  value: string;
  label: string;
}

/** Union catalog efforts with live thought_level choices; prefer live labels when connected. */
export function mergeCursorEffortChoices(
  catalogEfforts: readonly string[],
  liveChoices: ReadonlyArray<{ value: string; name: string }> = [],
): CursorEffortChoice[] {
  const values = new Set<string>();
  for (const effort of catalogEfforts) values.add(effort);
  for (const choice of liveChoices) values.add(choice.value);
  if (values.size <= 1) return [];
  return [...values]
    .sort((a, b) => effortRank(a) - effortRank(b))
    .map((value) => {
      const live = liveChoices.find((choice) => choice.value === value);
      return { value, label: live?.name ?? effortOptionLabel(value) };
    });
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
