/** History scroll restore when transcript content grows above the viewport. */

export const HISTORY_PRELOAD_PX = 200;
export const AUTO_LOAD_COOLDOWN_MS = 500;

export interface AutoLoadState {
  armed: boolean;
  lastLoadAt: number;
}

export function scrollHeightDelta(beforeScrollHeight: number, afterScrollHeight: number): number {
  return afterScrollHeight - beforeScrollHeight;
}

/** Drop restore when reveal was a no-op or layout did not grow above the viewport. */
export function anchorIsStale(
  beforeScrollHeight: number,
  afterScrollHeight: number,
  revealedCount: number,
): boolean {
  if (revealedCount <= 0) return true;
  return scrollHeightDelta(beforeScrollHeight, afterScrollHeight) <= 0;
}

/**
 * Near-top auto-load with overflow gate, arm/disarm, and cooldown so mount at
 * scrollTop 0 and restore nudges cannot cascade.
 */
export function autoLoadDecision(
  thread: HTMLDivElement,
  state: AutoLoadState,
  hasEarlier: boolean,
  now: number,
): { shouldLoad: boolean; nextState: AutoLoadState } {
  if (!hasEarlier) {
    return { shouldLoad: false, nextState: { armed: false, lastLoadAt: state.lastLoadAt } };
  }

  const { scrollTop, scrollHeight, clientHeight } = thread;
  if (scrollHeight <= clientHeight) {
    return { shouldLoad: false, nextState: { armed: false, lastLoadAt: state.lastLoadAt } };
  }

  let armed = state.armed;
  if (scrollTop > HISTORY_PRELOAD_PX) armed = true;

  const nearTop = scrollTop <= HISTORY_PRELOAD_PX;
  if (!nearTop) {
    return { shouldLoad: false, nextState: { armed, lastLoadAt: state.lastLoadAt } };
  }

  if (!armed) {
    return { shouldLoad: false, nextState: { armed, lastLoadAt: state.lastLoadAt } };
  }

  if (now - state.lastLoadAt < AUTO_LOAD_COOLDOWN_MS) {
    return { shouldLoad: false, nextState: { armed, lastLoadAt: state.lastLoadAt } };
  }

  return {
    shouldLoad: true,
    nextState: { armed: false, lastLoadAt: now },
  };
}

/**
 * Preserve read position after prepend-style growth at the top of the scroller.
 * Bottom-only growth while unpinned must not adjust scrollTop.
 */
export function restoreScrollAfterTopGrowth(
  thread: HTMLDivElement,
  beforeScrollTop: number,
  beforeScrollHeight: number,
): void {
  const delta = scrollHeightDelta(beforeScrollHeight, thread.scrollHeight);
  if (delta <= 0) return;
  thread.scrollTop = beforeScrollTop + delta;
}
