export const HISTORY_PRELOAD_PX = 200;
export const AUTO_LOAD_COOLDOWN_MS = 500;

export interface AutoLoadState {
  armed: boolean;
  lastLoadAt: number;
}

export function scrollHeightDelta(beforeScrollHeight: number, afterScrollHeight: number): number {
  return afterScrollHeight - beforeScrollHeight;
}

export function anchorIsStale(
  beforeScrollHeight: number,
  afterScrollHeight: number,
  revealedCount: number,
): boolean {
  if (revealedCount <= 0) return true;
  return scrollHeightDelta(beforeScrollHeight, afterScrollHeight) <= 0;
}

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

  // Reversed coords: live edge is scrollTop = 0; the history edge is the
  // most-negative offset (clientHeight - scrollHeight). The preload band and
  // arm/disarm are measured from that edge, not from zero.
  const minScrollTop = Math.min(0, clientHeight - scrollHeight);
  let armed = state.armed;
  if (scrollTop > minScrollTop + HISTORY_PRELOAD_PX) armed = true;

  const nearTop = scrollTop <= minScrollTop + HISTORY_PRELOAD_PX;
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

export function restoreScrollAfterTopGrowth(
  thread: HTMLDivElement,
  beforeScrollTop: number,
  beforeScrollHeight: number,
): void {
  const delta = scrollHeightDelta(beforeScrollHeight, thread.scrollHeight);
  if (delta <= 0) return;
  // Reversed coords: prepended rows grow distance from the live edge, so the
  // read position moves further negative by the delta. Pinned windows start at
  // 0 and land at -delta; a window that still fits clamps back to 0 natively.
  thread.scrollTop = beforeScrollTop - delta;
}
