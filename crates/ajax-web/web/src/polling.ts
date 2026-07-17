// Polling cadences and timeouts for the cockpit refresh and restart flows.

export const REFRESH_INTERVAL_ACTIVE_MS = 1000;
export const REFRESH_INTERVAL_TERMINAL_MS = 5000;
export const REFRESH_INTERVAL_IDLE_MS = 10000;
export const REFRESH_INTERVAL_HIDDEN_MS = 60000;

export const VERSION_POLL_MS = 30000;
export const VERSION_POLL_TERMINAL_MS = 120_000;
export const VERSION_POLL_HIDDEN_MS = 300_000;

export const CONFIRM_TIMEOUT_MS = 8000;
// Pre-commit undo window: a confirmed Drop waits this long before calling the
// API, giving the operator a chance to cancel from the result toast.
export const DROP_UNDO_MS = 5000;
export const RESULT_AUTO_DISMISS_MS = 12000;
// Success toasts (e.g. Drop) are informational — dismiss fast so they can't
// linger. Errors keep the longer window so failure output stays readable.
export const RESULT_SUCCESS_DISMISS_MS = 4000;
export const RESTART_POLL_MS = 500;
export const RESTART_TIMEOUT_MS = 30000;

export type PollingRouteKind = "dashboard" | "project" | "task" | "settings";

export function cockpitRefreshIntervalMs(input: {
  visibilityState: DocumentVisibilityState;
  routeKind: PollingRouteKind;
}): number {
  if (input.visibilityState !== "visible") return REFRESH_INTERVAL_HIDDEN_MS;
  if (input.routeKind === "task") return REFRESH_INTERVAL_TERMINAL_MS;
  if (input.routeKind === "settings") return REFRESH_INTERVAL_IDLE_MS;
  return REFRESH_INTERVAL_ACTIVE_MS;
}

export function versionPollIntervalMs(input: {
  visibilityState: DocumentVisibilityState;
  routeKind: PollingRouteKind;
}): number {
  if (input.visibilityState !== "visible") return VERSION_POLL_HIDDEN_MS;
  if (input.routeKind === "task") return VERSION_POLL_TERMINAL_MS;
  return VERSION_POLL_MS;
}
