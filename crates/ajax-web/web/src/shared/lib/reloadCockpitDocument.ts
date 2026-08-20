/** Query param on the shell document URL so navigation is not identical to the current one (#1007). */
export const COCKPIT_RELOAD_PARAM = "_r";

/** Load a fresh Cockpit shell document; hash-only replace and reload are no-ops (#1007, #1008). */
export function reloadCockpitDocument(location: Location = window.location): void {
  const url = new URL(location.href);
  url.searchParams.set(COCKPIT_RELOAD_PARAM, String(Date.now()));
  location.replace(url.toString());
}
