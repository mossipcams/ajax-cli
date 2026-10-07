export const REFRESH_INTERVAL_ACTIVE_MS = 3000;
export const REFRESH_INTERVAL_TERMINAL_MS = 10000;
export const REFRESH_INTERVAL_IDLE_MS = 10000;
export const REFRESH_INTERVAL_HIDDEN_MS = 60000;

export const VERSION_POLL_MS = 30000;
export const VERSION_POLL_TERMINAL_MS = 120_000;
export const VERSION_POLL_HIDDEN_MS = 300_000;

export const CONFIRM_TIMEOUT_MS = 8000;
export const DROP_UNDO_MS = 5000;
export const RESULT_AUTO_DISMISS_MS = 12000;
export const RESULT_SUCCESS_DISMISS_MS = 4000;
export const RESTART_POLL_MS = 500;
export const RESTART_TIMEOUT_MS = 30000;
export const TEST_IN_STABLE_TIMEOUT_MS = 900_000;
export const GET_REQUEST_TIMEOUT_MS = 10000;
export const DIFF_REQUEST_TIMEOUT_MS = 45000;

export type PollingRouteKind = "dashboard" | "project" | "task" | "diff" | "settings" | "session";

export function cockpitRefreshIntervalMs(input: {
  visibilityState: DocumentVisibilityState;
  routeKind: PollingRouteKind;
  fleetQuiet?: boolean;
}): number {
  if (input.visibilityState !== "visible") return REFRESH_INTERVAL_HIDDEN_MS;
  if (input.routeKind === "task" || input.routeKind === "session") {
    return REFRESH_INTERVAL_TERMINAL_MS;
  }
  if (input.routeKind === "settings" || input.routeKind === "diff") {
    return REFRESH_INTERVAL_IDLE_MS;
  }
  if (
    (input.routeKind === "dashboard" || input.routeKind === "project") &&
    input.fleetQuiet
  ) {
    return REFRESH_INTERVAL_IDLE_MS;
  }
  return REFRESH_INTERVAL_ACTIVE_MS;
}

export function versionPollIntervalMs(input: {
  visibilityState: DocumentVisibilityState;
  routeKind: PollingRouteKind;
}): number {
  if (input.visibilityState !== "visible") return VERSION_POLL_HIDDEN_MS;
  if (input.routeKind === "task" || input.routeKind === "session") {
    return VERSION_POLL_TERMINAL_MS;
  }
  return VERSION_POLL_MS;
}
