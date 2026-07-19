// Pure pull-to-refresh math. The component owns touch listeners and the timer;
// this module owns only the state transitions so they stay unit-testable.

export const PULL_THRESHOLD = 64; // resisted px at which the gesture arms
export const PULL_MAX = 96; // resisted px ceiling
const RESISTANCE = 0.5; // rubber-band factor applied to raw drag

export interface PullState {
  /** Whether the gesture began at the top of the scroll container. */
  active: boolean;
  /** Resisted, capped pull distance for the indicator. */
  distance: number;
  /** Past threshold — release will trigger a refresh. */
  armed: boolean;
}

export function pullStart(scrollTop: number): PullState {
  return { active: scrollTop <= 0, distance: 0, armed: false };
}

export function pullMove(state: PullState, rawDelta: number): PullState {
  if (!state.active || rawDelta <= 0) {
    return { ...state, distance: 0, armed: false };
  }
  const distance = Math.min(PULL_MAX, rawDelta * RESISTANCE);
  return { ...state, distance, armed: distance >= PULL_THRESHOLD };
}

export function pullEnd(state: PullState): { triggered: boolean } {
  return { triggered: state.active && state.armed };
}
