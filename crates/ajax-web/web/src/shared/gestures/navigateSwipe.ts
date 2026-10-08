export const NAVIGATE_SWIPE_TRIGGER = 56;
const ENGAGE_MIN = 48;
const LOCK_RATIO = 1.15;

export type NavigateSwipeDirection = "none" | "left" | "right";

export interface NavigateSwipeState {
  engaged: boolean;
  direction: NavigateSwipeDirection;
  dx: number;
  engageDx: number;
}

export function navigateSwipeStart(): NavigateSwipeState {
  return { engaged: false, direction: "none", dx: 0, engageDx: 0 };
}

export function crossSlideRemainingPx(
  direction: Exclude<NavigateSwipeDirection, "none">,
  dragX: number,
  pageWidth: number,
): number {
  return direction === "left" ? pageWidth + dragX : pageWidth - dragX;
}

export function crossSlideEnteringOffset(
  direction: Exclude<NavigateSwipeDirection, "none">,
  dragX: number,
  pageWidth: number,
): number {
  return direction === "left" ? pageWidth + dragX : -pageWidth + dragX;
}

export function crossSlideLeavingTarget(
  direction: Exclude<NavigateSwipeDirection, "none">,
  pageWidth: number,
): number {
  return direction === "left" ? -pageWidth : pageWidth;
}

export interface NavigateSwipeMoveOptions {
  capRightCommit?: boolean;
}

export function navigateSwipeMove(
  state: NavigateSwipeState,
  dx: number,
  dy: number,
  pageWidth: number,
  options: NavigateSwipeMoveOptions = {},
): NavigateSwipeState {
  const capRightCommit = options.capRightCommit ?? false;
  let engaged = state.engaged;
  let engageDx = state.engageDx;
  if (!engaged) {
    if (Math.abs(dx) < ENGAGE_MIN) return state;
    if (Math.abs(dx) <= Math.abs(dy) * LOCK_RATIO) {
      return navigateSwipeStart();
    }
    if (capRightCommit && dx > 0) return state;
    engaged = true;
    engageDx = dx;
  }
  const max = Math.max(pageWidth, NAVIGATE_SWIPE_TRIGGER);
  const travelDx = capRightCommit ? Math.min(dx, 0) : dx;
  const clamped = Math.max(-max, Math.min(max, travelDx));
  if (travelDx <= -NAVIGATE_SWIPE_TRIGGER) {
    return { engaged, direction: "left", dx: clamped, engageDx };
  }
  if (!capRightCommit && travelDx >= NAVIGATE_SWIPE_TRIGGER) {
    return { engaged, direction: "right", dx: clamped, engageDx };
  }
  return { engaged, direction: "none", dx: clamped, engageDx };
}

export function navigateSwipeEnd(
  state: NavigateSwipeState,
  options: NavigateSwipeMoveOptions = {},
): NavigateSwipeDirection {
  if (!state.engaged) return "none";
  if (options.capRightCommit && state.direction === "right") return "none";
  return state.direction;
}

export function navigateSwipeTranslateX(state: NavigateSwipeState): number {
  if (!state.engaged) return 0;
  return state.dx - state.engageDx;
}

export function navigateSwipeCommitOffset(
  direction: Exclude<NavigateSwipeDirection, "none">,
  pageWidth: number,
): number {
  return direction === "left" ? -pageWidth : pageWidth;
}

export function isDiffPanGestureTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('[data-testid="diff-pr-strip"]') ||
      target.closest('[data-testid="diff-hunk"]') ||
      target.closest('[data-testid="diff-hunk-viewer"]') ||
      target.closest(".diff-hunk"),
  );
}
