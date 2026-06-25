// Visibility- and state-aware pane polling cadence. Pure function extracted
// from the legacy `paneInterval`; timer scheduling stays in the caller.

export const PANE_INTERVALS = {
  default: 1000,
  unchanged: 2500,
  idle: 4000,
} as const;

export const REFRESH_INTERVAL_MS = 1000;
export const VERSION_POLL_MS = 30000;
export const CONFIRM_TIMEOUT_MS = 8000;
export const RESULT_AUTO_DISMISS_MS = 12000;
export const RESTART_POLL_MS = 500;
export const RESTART_TIMEOUT_MS = 30000;
export const MAX_LOG_ENTRIES = 24;

export interface PaneIntervalInput {
  hidden: boolean;
  stateKind: string | undefined;
}

export function paneInterval({ hidden, stateKind }: PaneIntervalInput): number {
  if (hidden) return PANE_INTERVALS.idle;
  if (!stateKind) return PANE_INTERVALS.default;
  if (
    stateKind === "WaitingForApproval" ||
    stateKind === "WaitingForInput" ||
    stateKind === "AgentRunning"
  ) {
    return PANE_INTERVALS.default;
  }
  if (stateKind === "Done" || stateKind === "Idle") {
    return PANE_INTERVALS.idle;
  }
  return PANE_INTERVALS.unchanged;
}
