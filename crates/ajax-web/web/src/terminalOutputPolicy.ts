/**
 * Pure output-follow and resize-size decisions for the mobile terminal.
 *
 * Keeps scrollback compensation, pinned/unseen follow effects, and resize
 * size validation unit-testable without Ghostty, WebSocket, or DOM.
 */

/** Scroll delta to preserve reader position when scrollback grows. */
export function scrollbackGrowthCompensation(before: number, after: number): number {
  if (!Number.isFinite(before) || !Number.isFinite(after) || !(after > before)) {
    return 0;
  }
  return before - after;
}

/** Follow/unseen effects for a write given pinned-to-bottom state. */
export function outputFollowEffects(pinnedToBottom: boolean): {
  snapToBottom: boolean;
  markUnseenOutput: boolean;
} {
  if (pinnedToBottom) {
    return { snapToBottom: true, markUnseenOutput: false };
  }
  return { snapToBottom: false, markUnseenOutput: true };
}

/** Accept only finite positive integer cols/rows; otherwise fail closed. */
export function validTerminalSize(
  cols: number,
  rows: number,
): { cols: number; rows: number } | undefined {
  if (
    !Number.isFinite(cols) ||
    !Number.isFinite(rows) ||
    !Number.isInteger(cols) ||
    !Number.isInteger(rows) ||
    cols <= 0 ||
    rows <= 0
  ) {
    return undefined;
  }
  return { cols, rows };
}
