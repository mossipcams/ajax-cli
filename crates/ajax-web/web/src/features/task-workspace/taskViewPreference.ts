export const TASK_TERMINAL_PREFERENCE_STORAGE_KEY = "ajax.web.taskView.terminal";

const TASK_VIEW_EVENT = "ajax:task-view";

function readPreferenceSet(): Set<string> {
  try {
    const raw = localStorage.getItem(TASK_TERMINAL_PREFERENCE_STORAGE_KEY);
    if (!raw) return new Set();
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((h): h is string => typeof h === "string" && h.length > 0));
  } catch {
    return new Set();
  }
}

function writePreferenceSet(handles: Set<string>): void {
  try {
    localStorage.setItem(TASK_TERMINAL_PREFERENCE_STORAGE_KEY, JSON.stringify([...handles]));
    window.dispatchEvent(new CustomEvent(TASK_VIEW_EVENT));
  } catch {
    // Private mode / storage denied: preference just won't stick.
  }
}

export function readTaskTerminalPreferred(handle: string): boolean {
  if (!handle) return false;
  return readPreferenceSet().has(handle);
}

export function writeTaskTerminalPreferred(handle: string): void {
  if (!handle) return;
  const next = readPreferenceSet();
  next.add(handle);
  writePreferenceSet(next);
}

export function clearTaskTerminalPreferred(handle: string): void {
  if (!handle) return;
  const next = readPreferenceSet();
  if (!next.delete(handle)) return;
  writePreferenceSet(next);
}

export function subscribeTaskViewPreference(listener: () => void): () => void {
  const onStorage = (event: StorageEvent) => {
    if (event.key === TASK_TERMINAL_PREFERENCE_STORAGE_KEY) listener();
  };
  const onCustom = () => listener();
  window.addEventListener("storage", onStorage);
  window.addEventListener(TASK_VIEW_EVENT, onCustom);
  return () => {
    window.removeEventListener("storage", onStorage);
    window.removeEventListener(TASK_VIEW_EVENT, onCustom);
  };
}
