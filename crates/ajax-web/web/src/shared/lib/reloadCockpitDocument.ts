export const COCKPIT_RELOAD_PARAM = "_r";

export const COCKPIT_RELOAD_WATCH_MS = 2_000;

export type ReloadCockpitDocumentOptions = {
  onNavigationMissed?: () => void;
};

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
