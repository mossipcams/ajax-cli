// Pure swipe-to-reveal math for list rows. A left swipe slides the row to
// expose one action behind it; the component owns the touch listeners and the
// transform. Vertical-dominant drags are ignored so the list keeps scrolling.

export const SWIPE_REVEAL_WIDTH = 158; // px the revealed action occupies
/** Synced to `.task-row-reveal` width in `styles/task/list.css`. */
export const SWIPE_REVEAL_WIDTH_VAR = "--task-row-reveal-width";
export const SWIPE_TRIGGER = 56; // px past which release snaps open
/** Settled-open reveal auto-closes after this unless confirm is pending. */
export const REVEAL_AUTO_HIDE_MS = 10_000;
const ENGAGE_MIN = 8; // px of horizontal travel before deciding intent
const LOCK_RATIO = 1.2; // |dx| must beat |dy| * ratio to engage horizontally

export interface SwipeState {
  /** Horizontal intent confirmed; once true the row tracks the finger. */
  engaged: boolean;
  /** Reveal offset in px (0..SWIPE_REVEAL_WIDTH). */
  offset: number;
  /** Currently past the snap-open trigger. */
  open: boolean;
  /** Offset when the touch began — tracks close-from-open via swipe right. */
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
