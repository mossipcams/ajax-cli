// Pure horizontal navigate-swipe math for task chrome. Left opens Diff Review;
// right returns. Vertical-dominant drags stay ignored so scroll still works.

export const NAVIGATE_SWIPE_TRIGGER = 64; // px past which release navigates
const ENGAGE_MIN = 10;
const LOCK_RATIO = 1.2;

export type NavigateSwipeDirection = "none" | "left" | "right";

export interface NavigateSwipeState {
  engaged: boolean;
  direction: NavigateSwipeDirection;
}

export function navigateSwipeStart(): NavigateSwipeState {
  return { engaged: false, direction: "none" };
}

export function navigateSwipeMove(
  state: NavigateSwipeState,
  dx: number,
  dy: number,
): NavigateSwipeState {
  let engaged = state.engaged;
  if (!engaged) {
    if (Math.abs(dx) < ENGAGE_MIN) return state;
    if (Math.abs(dx) <= Math.abs(dy) * LOCK_RATIO) {
      return { engaged: false, direction: "none" };
    }
    engaged = true;
  }
  if (dx <= -NAVIGATE_SWIPE_TRIGGER) return { engaged, direction: "left" };
  if (dx >= NAVIGATE_SWIPE_TRIGGER) return { engaged, direction: "right" };
  return { engaged, direction: "none" };
}

export function navigateSwipeEnd(state: NavigateSwipeState): NavigateSwipeDirection {
  return state.engaged ? state.direction : "none";
}

/** True when the touch target lives on the terminal interaction surface. */
export function isTerminalGestureTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('[data-testid="terminal-interaction-surface"]') ||
      target.closest('[data-testid="task-terminal-panel"]'),
  );
}

/** True when Diff Review horizontal pans (chips / hunks) own the gesture. */
export function isDiffPanGestureTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('[data-testid="diff-pr-strip"]') ||
      target.closest('[data-testid="diff-hunk"]') ||
      target.closest('[data-testid="diff-hunk-viewer"]') ||
      target.closest(".diff-hunk"),
  );
}
