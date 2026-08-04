/** Document flags so page-swipe (capture) can ignore terminal select gestures. */
const TERMINAL_SELECTING_DATASET = "ajaxTerminalSelecting";
const TERMINAL_DOUBLE_TAP_PENDING_DATASET = "ajaxTerminalDoubleTapPending";

/** Surfaces that own double-tap / long-press terminal selection. */
const TERMINAL_TOUCH_SELECTOR = ".terminal-host, .terminal-interaction-wrap, .xterm";

export function setTerminalSelecting(active: boolean): void {
  if (active) {
    document.documentElement.dataset[TERMINAL_SELECTING_DATASET] = "1";
  } else {
    delete document.documentElement.dataset[TERMINAL_SELECTING_DATASET];
  }
}

export function isTerminalSelecting(): boolean {
  return document.documentElement.dataset[TERMINAL_SELECTING_DATASET] === "1";
}

export function setTerminalDoubleTapPending(active: boolean): void {
  if (active) {
    document.documentElement.dataset[TERMINAL_DOUBLE_TAP_PENDING_DATASET] = "1";
  } else {
    delete document.documentElement.dataset[TERMINAL_DOUBLE_TAP_PENDING_DATASET];
  }
}

export function isTerminalDoubleTapPending(): boolean {
  return document.documentElement.dataset[TERMINAL_DOUBLE_TAP_PENDING_DATASET] === "1";
}

export function isTerminalTouchTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(target.closest(TERMINAL_TOUCH_SELECTOR));
}

/** True when page swipe must not arm or continue for this touch target. */
export function shouldSuppressPageSwipe(target: EventTarget | null = null): boolean {
  if (isTerminalSelecting()) return true;
  return isTerminalDoubleTapPending() && isTerminalTouchTarget(target);
}
