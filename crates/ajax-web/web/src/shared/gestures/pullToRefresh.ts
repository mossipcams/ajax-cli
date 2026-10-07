export const PULL_THRESHOLD = 64;
export const PULL_MAX = 96;
const RESISTANCE = 0.5;

export interface PullState {
  active: boolean;
  distance: number;
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
