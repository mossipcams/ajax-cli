/** Query param on the shell document URL so navigation is not identical to the current one (#1007). */
export const COCKPIT_RELOAD_PARAM = "_r";

/** How long to wait before treating a shell replace as a no-op (#1007). */
export const COCKPIT_RELOAD_WATCH_MS = 2_000;

export type ReloadCockpitDocumentOptions = {
  /** Called when the document URL is unchanged after the watch interval. */
  onNavigationMissed?: () => void;
};

/** Load a fresh Cockpit shell document; hash-only replace and reload are no-ops (#1007, #1008). */
export function reloadCockpitDocument(
  location: Location = window.location,
  options?: ReloadCockpitDocumentOptions,
): boolean {
  const hrefBefore = location.href;
  const url = new URL(location.href);
  url.searchParams.set(COCKPIT_RELOAD_PARAM, String(Date.now()));
  try {
    location.replace(url.toString());
  } catch {
    return false;
  }
  const onMissed = options?.onNavigationMissed;
  if (onMissed) {
    window.setTimeout(() => {
      if (location.href === hrefBefore) onMissed();
    }, COCKPIT_RELOAD_WATCH_MS);
  }
  return true;
}
