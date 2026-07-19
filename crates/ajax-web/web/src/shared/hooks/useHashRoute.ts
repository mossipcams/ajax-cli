import { useMemo, useSyncExternalStore } from "react";
import { parseRoute, type Route } from "@/shared/lib/routes";

// The URL hash is external state React does not own, so it is read as a store
// rather than copied into component state through an effect. `subscribe` and
// `getSnapshot` live at module scope so their identities are stable and
// useSyncExternalStore never re-subscribes.
function subscribe(onChange: () => void): () => void {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

// The snapshot must be the raw hash string, not a parsed Route. Snapshots are
// compared by identity, and returning a freshly built object here would make
// every check report a change and loop forever.
function getSnapshot(): string {
  return window.location.hash;
}

function getServerSnapshot(): string {
  return "#/";
}

export function useHashRoute(): Route {
  const hash = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
  // Parsing is cheap, but the identity is load-bearing: App holds
  // `useEffect(..., [route])` for the document title, which would fire on every
  // render if this rebuilt the object each time.
  return useMemo(() => parseRoute(hash), [hash]);
}
