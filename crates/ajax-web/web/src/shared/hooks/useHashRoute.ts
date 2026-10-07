import { useMemo, useSyncExternalStore } from "react";
import { parseRoute, type Route } from "@/shared/lib/routes";

function subscribe(onChange: () => void): () => void {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

function getSnapshot(): string {
  return window.location.hash;
}

function getServerSnapshot(): string {
  return "#/";
}

export function useHashRoute(): Route {
  const hash = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
  return useMemo(() => parseRoute(hash), [hash]);
}
