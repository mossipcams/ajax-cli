import { useCallback, useEffect, useState } from "react";

export const SESSION_MODEL_STORAGE_KEY = "ajax.web.session.model";
export const DEFAULT_SESSION_MODEL = "auto";

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
  try {
    const response = await fetch(`/api/session/models?agent=${encodeURIComponent(agent)}`, {
      credentials: "same-origin",
    });
    if (!response.ok) return fallbackCatalog(agent);
    const body = (await response.json()) as {
      models?: SessionModelOption[];
      default?: string;
      reasoning?: SessionModelGroup;
      error?: string;
    };
    if (!Array.isArray(body.models) || body.models.length === 0) {
      return body.error
        ? { models: [], default: "", error: body.error }
        : fallbackCatalog(agent);
    }
    return {
      models: body.models.filter(
        (m) => m && typeof m.id === "string" && typeof m.label === "string",
      ),
      default: typeof body.default === "string" ? body.default : "",
      ...(body.reasoning && Array.isArray(body.reasoning.options)
        ? { reasoning: body.reasoning }
        : {}),
    };
  } catch {
    return fallbackCatalog(agent);
  }
}
