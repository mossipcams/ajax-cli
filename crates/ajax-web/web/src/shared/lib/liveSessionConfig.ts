import {
  DEFAULT_SESSION_MODEL,
  encodeModelSelection,
} from "@/shared/lib/sessionModelSelection";

export interface LiveConfigOptionChoice {
  value: string;
  name: string;
}

/** AoE-style descriptor mirrored from protocol v2 `sessionConfigOptions`. */
export interface LiveSessionConfigOption {
  id: string;
  category?: string;
  name: string;
  type: string;
  currentValue: string | boolean;
  choices: LiveConfigOptionChoice[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object";
}

export function parseLiveConfigOptions(raw: unknown): LiveSessionConfigOption[] | undefined {
  if (!Array.isArray(raw) || raw.length === 0) return undefined;
  const parsed: LiveSessionConfigOption[] = [];
  for (const item of raw) {
    if (!isRecord(item) || typeof item.id !== "string" || typeof item.name !== "string") {
      return undefined;
    }
    const kind = typeof item.type === "string" ? item.type : "";
    const current = item.currentValue;
    if (typeof current !== "string" && typeof current !== "boolean") return undefined;
    const choices: LiveConfigOptionChoice[] = [];
    if (Array.isArray(item.choices)) {
      for (const choice of item.choices) {
        if (
          isRecord(choice) &&
          typeof choice.value === "string" &&
          typeof choice.name === "string"
        ) {
          choices.push({ value: choice.value, name: choice.name });
        }
      }
    }
    parsed.push({
      id: item.id,
      ...(typeof item.category === "string" ? { category: item.category } : {}),
      name: item.name,
      type: kind,
      currentValue: current,
      choices,
    });
  }
  return parsed.length ? parsed : undefined;
}

export function findLiveOptionByCategory(
  options: LiveSessionConfigOption[],
  category: string,
  fallbackIds: string[],
): LiveSessionConfigOption | undefined {
  const byCategory = options.find((option) => option.category === category);
  if (byCategory) return byCategory;
  const id = fallbackIds.find((candidate) => options.some((option) => option.id === candidate));
  return id ? options.find((option) => option.id === id) : undefined;
}

export function modelLiveOption(
  options: LiveSessionConfigOption[],
): LiveSessionConfigOption | undefined {
  return findLiveOptionByCategory(options, "model", ["model"]);
}

export function thoughtLevelLiveOption(
  options: LiveSessionConfigOption[],
): LiveSessionConfigOption | undefined {
  return findLiveOptionByCategory(options, "thought_level", ["reasoning", "effort"]);
}

export function modelConfigBooleanLiveOption(
  options: LiveSessionConfigOption[],
): LiveSessionConfigOption | undefined {
  const byCategory = options.find(
    (option) => option.category === "model_config" && option.type === "boolean",
  );
  if (byCategory) return byCategory;
  return options.find((option) => option.id === "fast" && option.type === "boolean");
}

export function readLiveSelectCurrent(option: LiveSessionConfigOption): string | undefined {
  return typeof option.currentValue === "string" && option.currentValue
    ? option.currentValue
    : undefined;
}

export function readLiveBooleanCurrent(option: LiveSessionConfigOption): boolean | undefined {
  return typeof option.currentValue === "boolean" ? option.currentValue : undefined;
}

/** Build the Ajax desired pin from live advertised current values. */
export function encodeDesiredPinFromLiveOptions(options: LiveSessionConfigOption[]): string {
  const model = modelLiveOption(options);
  const base = model ? readLiveSelectCurrent(model) ?? "" : "";
  if (!base || base === DEFAULT_SESSION_MODEL) return DEFAULT_SESSION_MODEL;

  const extras: Record<string, string> = {};
  const thought = thoughtLevelLiveOption(options);
  if (thought) {
    const level = readLiveSelectCurrent(thought);
    if (level) extras[thought.id] = level;
  }
  const fast = modelConfigBooleanLiveOption(options);
  if (fast) {
    const on = readLiveBooleanCurrent(fast);
    if (on !== undefined) extras[fast.id] = on ? "true" : "false";
  }
  return encodeModelSelection(base, extras);
}

/** Encode a desired pin after the operator changes one live advertised control. */
export function encodeDesiredPinWithLiveSelection(
  options: LiveSessionConfigOption[],
  selection: {
    model?: string;
    thoughtLevel?: string;
    fast?: boolean;
  },
): string {
  const modelOpt = modelLiveOption(options);
  const base =
    selection.model ??
    (modelOpt ? readLiveSelectCurrent(modelOpt) : undefined) ??
    DEFAULT_SESSION_MODEL;
  if (!base || base === DEFAULT_SESSION_MODEL) return DEFAULT_SESSION_MODEL;

  const extras: Record<string, string> = {};
  const thought = thoughtLevelLiveOption(options);
  if (thought) {
    const level =
      selection.thoughtLevel ?? readLiveSelectCurrent(thought);
    if (level) extras[thought.id] = level;
  }
  const fastOpt = modelConfigBooleanLiveOption(options);
  if (fastOpt) {
    const on = selection.fast ?? readLiveBooleanCurrent(fastOpt) ?? false;
    extras[fastOpt.id] = on ? "true" : "false";
  }
  return encodeModelSelection(base, extras);
}
