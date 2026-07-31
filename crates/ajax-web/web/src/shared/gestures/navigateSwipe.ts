// Pure horizontal navigate-swipe math for task chrome. Task detail: swipe left
// opens Diff Review; Diff Review: swipe right returns. Vertical-dominant drags
// stay ignored so scroll still works.

export const NAVIGATE_SWIPE_TRIGGER = 56; // px past which release navigates
export const NAVIGATE_SWIPE_MAX = 96; // visual clamp while dragging
const ENGAGE_MIN = 8;
const LOCK_RATIO = 1.15;

export type NavigateSwipeDirection = "none" | "left" | "right";

export interface NavigateSwipeState {
  engaged: boolean;
  direction: NavigateSwipeDirection;
  /** Signed horizontal travel (negative = left). */
  dx: number;
}

export function navigateSwipeStart(): NavigateSwipeState {
  return { engaged: false, direction: "none", dx: 0 };
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
      return { engaged: false, direction: "none", dx: 0 };
    }
    engaged = true;
  }
  const clamped = Math.max(-NAVIGATE_SWIPE_MAX, Math.min(NAVIGATE_SWIPE_MAX, dx));
  if (dx <= -NAVIGATE_SWIPE_TRIGGER) {
    return { engaged, direction: "left", dx: clamped };
  }
  if (dx >= NAVIGATE_SWIPE_TRIGGER) {
    return { engaged, direction: "right", dx: clamped };
  }
  return { engaged, direction: "none", dx: clamped };
}

export function navigateSwipeEnd(state: NavigateSwipeState): NavigateSwipeDirection {
  return state.engaged ? state.direction : "none";
}

/** Visual translate for left-open / right-back feedback while dragging. */
export function navigateSwipeTranslateX(state: NavigateSwipeState): number {
  if (!state.engaged) return 0;
  return state.dx;
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
