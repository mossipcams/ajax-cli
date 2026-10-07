export const SWIPE_REVEAL_WIDTH = 158;
export const SWIPE_REVEAL_WIDTH_VAR = "--task-row-reveal-width";
export const SWIPE_TRIGGER = 56;
export const REVEAL_AUTO_HIDE_MS = 10_000;
const ENGAGE_MIN = 8;
const LOCK_RATIO = 1.2;

export interface SwipeState {
  engaged: boolean;
  offset: number;
  open: boolean;
  baseOffset: number;
}

export function swipeStart(initialOffset = 0): SwipeState {
  const baseOffset = Math.min(SWIPE_REVEAL_WIDTH, Math.max(0, initialOffset));
  return {
    engaged: false,
    offset: baseOffset,
    open: baseOffset >= SWIPE_TRIGGER,
    baseOffset,
  };
}

export function swipeMove(state: SwipeState, dx: number, dy: number): SwipeState {
  let engaged = state.engaged;
  const baseOffset = state.baseOffset;
  if (!engaged) {
    if (Math.abs(dx) < ENGAGE_MIN) return state;
    if (Math.abs(dx) <= Math.abs(dy) * LOCK_RATIO) return { ...state, engaged: false };
    engaged = true;
  }
  const offset = Math.min(SWIPE_REVEAL_WIDTH, Math.max(0, baseOffset - dx));
  return { engaged, offset, open: offset >= SWIPE_TRIGGER, baseOffset };
}

export function swipeEnd(state: SwipeState): { open: boolean; offset: number } {
  const open = state.engaged && state.open;
  return { open, offset: open ? SWIPE_REVEAL_WIDTH : 0 };
}
