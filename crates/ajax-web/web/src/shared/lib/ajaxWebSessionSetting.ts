const STORAGE_KEY = "ajax.webSession";

export function isAjaxWebSessionEnabled(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

export function setAjaxWebSessionEnabled(enabled: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, enabled ? "true" : "false");
  } catch {
    // ponytail: Safari private mode may block localStorage writes.
  }
}
