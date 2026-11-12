export const EDGE_ZONE_PX = 50;
export const COMMIT_FLICK_VELOCITY = 0.2;
export const COMMIT_DISTANCE_FRACTION = 0.5;
const ENGAGE_MIN = 48;
const EDGE_ENGAGE_MIN = 10;
const LOCK_RATIO = 1.15;

export type NavigateSwipeDirection = "none" | "left" | "right";

export interface NavigateSwipeState {
  engaged: boolean;
  direction: NavigateSwipeDirection;
  dx: number;
  engageDx: number;
  edgeStart: boolean;
}

export function navigateSwipeStart(edgeStart = false): NavigateSwipeState {
  return { engaged: false, direction: "none", dx: 0, engageDx: 0, edgeStart };
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
    const engageMin = state.edgeStart && dx > 0 ? EDGE_ENGAGE_MIN : ENGAGE_MIN;
    if (Math.abs(dx) < engageMin) return state;
    if (Math.abs(dx) <= Math.abs(dy) * LOCK_RATIO) {
      return navigateSwipeStart(state.edgeStart);
    }
    if (capRightCommit && dx > 0) return state;
    engaged = true;
    engageDx = dx;
  }
  const travelDx = capRightCommit ? Math.min(dx, 0) : dx;
  const clamped = Math.max(-pageWidth, Math.min(pageWidth, travelDx));
  let direction: NavigateSwipeDirection = "none";
  if (clamped < 0) direction = "left";
  else if (clamped > 0 && !capRightCommit) direction = "right";
  return { engaged, direction, dx: clamped, engageDx, edgeStart: state.edgeStart };
}

export interface NavigateSwipeEndOptions {
  capRightCommit?: boolean;
  pageWidth: number;
  releaseVelocity: number;
}

export function navigateSwipeEnd(
  state: NavigateSwipeState,
  options: NavigateSwipeEndOptions,
): NavigateSwipeDirection {
  if (!state.engaged || state.dx === 0) return "none";
  const direction: Exclude<NavigateSwipeDirection, "none"> = state.dx < 0 ? "left" : "right";
  if (options.capRightCommit && direction === "right") return "none";
  const toward = direction === "left" ? -options.releaseVelocity : options.releaseVelocity;
  if (toward < 0) return "none";
  if (
    toward > COMMIT_FLICK_VELOCITY ||
    Math.abs(navigateSwipeTranslateX(state)) > options.pageWidth * COMMIT_DISTANCE_FRACTION
  ) {
    return direction;
  }
  return "none";
}

export function releaseVelocity(
  samples: ReadonlyArray<{ t: number; x: number }>,
  now: number,
  windowMs = 100,
): number {
  const recent = samples.filter((sample) => sample.t >= now - windowMs);
  if (recent.length < 2) return 0;
  const first = recent[0];
  const last = recent[recent.length - 1];
  return (last.x - first.x) / Math.max(last.t - first.t, 1);
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
