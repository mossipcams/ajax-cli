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

export async function fetchSessionModels(): Promise<SessionModelOption[]> {
  try {
    const response = await fetch("/api/session/models", { credentials: "same-origin" });
    if (!response.ok) return [{ id: DEFAULT_SESSION_MODEL, label: "Auto" }];
    const body = (await response.json()) as { models?: SessionModelOption[] };
    if (!Array.isArray(body.models) || body.models.length === 0) {
      return [{ id: DEFAULT_SESSION_MODEL, label: "Auto" }];
    }
    return body.models.filter((m) => m && typeof m.id === "string" && typeof m.label === "string");
  } catch {
    return [{ id: DEFAULT_SESSION_MODEL, label: "Auto" }];
  }
}
