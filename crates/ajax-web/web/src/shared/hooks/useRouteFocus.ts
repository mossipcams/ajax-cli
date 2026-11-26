import { useEffect, useRef } from "react";
import type { Route } from "@/shared/lib/routes";

function findRow(handle: string): HTMLElement | undefined {
  return Array.from(document.querySelectorAll<HTMLElement>(".task-row")).find(
    (row) => row.dataset.handle === handle,
  );
}

function focusRoute(route: Route, originHandle: string | undefined): void {
  const outlet = document.querySelector<HTMLElement>(`[data-outlet="${route.kind}"]`);
  if (!outlet || outlet.contains(document.activeElement)) return;

  const row =
    (route.kind === "dashboard" || route.kind === "project") && originHandle
      ? findRow(originHandle)
      : undefined;
  if (row) {
    row.focus({ preventScroll: true });
    return;
  }
  outlet.tabIndex = -1;
  outlet.focus({ preventScroll: true });
}

export function useRouteFocus(route: Route, transitioning: boolean): void {
  const seenRef = useRef<Route | null>(null);
  const pendingRef = useRef<{ originHandle: string | undefined } | null>(null);

  useEffect(() => {
    if (seenRef.current !== route) {
      if (seenRef.current) pendingRef.current = { originHandle: seenRef.current.handle };
      seenRef.current = route;
    }
    if (transitioning || !pendingRef.current) return;
    const { originHandle } = pendingRef.current;
    pendingRef.current = null;
    focusRoute(route, originHandle);
  }, [route, transitioning]);
}
