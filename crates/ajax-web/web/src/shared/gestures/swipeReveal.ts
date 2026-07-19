// Pure swipe-to-reveal math for list rows. A left swipe slides the row to
// expose one action behind it; the component owns the touch listeners and the
// transform. Vertical-dominant drags are ignored so the list keeps scrolling.

export const SWIPE_REVEAL_WIDTH = 88; // px the revealed action occupies
export const SWIPE_TRIGGER = 56; // px past which release snaps open
const ENGAGE_MIN = 8; // px of horizontal travel before deciding intent
const LOCK_RATIO = 1.2; // |dx| must beat |dy| * ratio to engage horizontally

export interface SwipeState {
  /** Horizontal intent confirmed; once true the row tracks the finger. */
  engaged: boolean;
  /** Reveal offset in px (0..SWIPE_REVEAL_WIDTH). */
  offset: number;
  /** Currently past the snap-open trigger. */
  open: boolean;
}

export function swipeStart(): SwipeState {
  return { engaged: false, offset: 0, open: false };
}

export function swipeMove(state: SwipeState, dx: number, dy: number): SwipeState {
  let engaged = state.engaged;
  if (!engaged) {
    if (Math.abs(dx) < ENGAGE_MIN) return state;
    if (Math.abs(dx) <= Math.abs(dy) * LOCK_RATIO) return { ...state, engaged: false };
    engaged = true;
  }
  const offset = Math.min(SWIPE_REVEAL_WIDTH, Math.max(0, -dx));
  return { engaged, offset, open: offset >= SWIPE_TRIGGER };
}

export function swipeEnd(state: SwipeState): { open: boolean; offset: number } {
  const open = state.engaged && state.open;
  return { open, offset: open ? SWIPE_REVEAL_WIDTH : 0 };
}
