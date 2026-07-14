const STORAGE_KEY = "ajax.terminal.surfaceV2";

const listeners = new Set<(enabled: boolean) => void>();
let storageListenerAttached = false;

function readEnabled(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function notify(enabled: boolean): void {
  for (const listener of listeners) {
    listener(enabled);
  }
}

function ensureStorageListener(): void {
  if (storageListenerAttached || typeof window === "undefined") return;
  storageListenerAttached = true;
  window.addEventListener("storage", (event) => {
    if (event.key !== STORAGE_KEY) return;
    notify(readEnabled());
  });
}

export function isTerminalSurfaceV2Enabled(): boolean {
  return readEnabled();
}

export function setTerminalSurfaceV2Enabled(enabled: boolean): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, enabled ? "true" : "false");
  } catch {
    // Best-effort: Safari private mode may throw.
  }
  notify(enabled);
}

export function subscribeTerminalSurfaceV2(
  listener: (enabled: boolean) => void,
): () => void {
  ensureStorageListener();
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
