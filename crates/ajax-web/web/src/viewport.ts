/**
 * Keyboard-aware viewport sync for the mobile terminal (iOS Safari first).
 *
 * iOS Safari does not honour `interactive-widget=resizes-content`, so the soft
 * keyboard never shrinks the layout viewport — it only shrinks `visualViewport`.
 * We mirror `visualViewport.height` into the `--app-height` CSS variable so a
 * fixed, full-screen terminal layer can size itself to the truly-visible band
 * above the keyboard, and toggle a `keyboard-open` class for layout that needs
 * it. Ported from the Codeman project's mobile-handlers.js.
 */

// Keyboard show/hide thresholds. The 100px close threshold (vs 50) absorbs iOS
// address-bar drift and the iOS 26 ~24px visual/layout discrepancy.
const KEYBOARD_OPEN_DELTA_PX = 150;
const KEYBOARD_CLOSE_DELTA_PX = 100;
const KEYBOARD_OPEN_CLASS = "keyboard-open";
const APP_HEIGHT_VAR = "--app-height";
const APP_TOP_VAR = "--app-top";

/**
 * The single keyboard-open truth. `initViewport` maintains the class with
 * baseline rebasing and open/close hysteresis; every consumer (CSS takeover,
 * the terminal's PTY-lockstep freeze) must read this same state so they can
 * never disagree about whether the keyboard is up.
 */
export function isKeyboardOpen(): boolean {
  return (
    typeof document !== "undefined" &&
    document.documentElement.classList.contains(KEYBOARD_OPEN_CLASS)
  );
}

/**
 * Clear document/window scroll offsets that Safari leaves behind after
 * keyboard or expand snaps. Safe in jsdom where `scrollTo` is unimplemented.
 */
export function resetDocumentScroll(): void {
  try {
    window.scrollTo(0, 0);
  } catch {
    // jsdom throws "Not implemented" for scrollTo.
  }
  document.documentElement.scrollTop = 0;
  document.body.scrollTop = 0;
  const scroller = document.scrollingElement;
  if (scroller) scroller.scrollTop = 0;
}

/**
 * Begin syncing `--app-height` / `keyboard-open` from `visualViewport`.
 * No-ops where `visualViewport` is unavailable. Returns a cleanup function
 * that removes every listener and the state it set.
 */
export function initViewport(): () => void {
  const vv = typeof window !== "undefined" ? window.visualViewport : undefined;
  if (!vv) return () => {};

  const root = document.documentElement;
  let baselineHeight = vv.height;
  let baselineWidth = window.innerWidth;
  let keyboardOpen = false;

  const setAppHeight = (height: number) => {
    root.style.setProperty(APP_HEIGHT_VAR, `${height}px`);
  };
  const setAppTop = (offsetTop: number) => {
    root.style.setProperty(APP_TOP_VAR, `${offsetTop}px`);
  };

  const syncViewportGeometry = () => {
    setAppHeight(vv.height);
    setAppTop(vv.offsetTop ?? 0);
  };
  syncViewportGeometry();

  const onViewportResize = () => {
    const current = vv.height;
    const currentWidth = window.innerWidth;
    if (currentWidth !== baselineWidth) {
      keyboardOpen = false;
      root.classList.remove(KEYBOARD_OPEN_CLASS);
      syncViewportGeometry();
      baselineHeight = current;
      baselineWidth = currentWidth;
      return;
    }
    const delta = baselineHeight - current;
    if (delta > KEYBOARD_OPEN_DELTA_PX && !keyboardOpen) {
      keyboardOpen = true;
      root.classList.add(KEYBOARD_OPEN_CLASS);
      resetDocumentScroll();
    } else if (delta < KEYBOARD_CLOSE_DELTA_PX && keyboardOpen) {
      keyboardOpen = false;
      root.classList.remove(KEYBOARD_OPEN_CLASS);
      resetDocumentScroll();
    }
    // Keep --app-height pinned to the visible band. While the keyboard is closed
    // this also tracks address-bar / orientation changes and re-bases the
    // threshold so the next keyboard open is measured from the right height.
    syncViewportGeometry();
    if (!keyboardOpen) {
      baselineHeight = current;
      baselineWidth = currentWidth;
    }
  };

  // Suppress pinch / double-tap zoom (iOS ignores user-scalable=no since iOS 10).
  const onGesture = (event: Event) => event.preventDefault();

  const onTouchMovePinchGuard = (event: TouchEvent) => {
    const scale = (event as TouchEvent & { scale?: number }).scale;
    if (typeof scale === "number" && scale !== 1) {
      event.preventDefault();
    }
  };

  // Two-finger touches have no legitimate page-level use in this app;
  // preventing the touchstart stops iOS from ever latching the zoom gesture
  // (the touchmove scale guard alone runs too late on PWA). preventDefault
  // does NOT stop event delivery, so the terminal host's own pinch handling
  // still receives the events.
  const onTouchStartPinchGuard = (event: TouchEvent) => {
    if (event.touches && event.touches.length >= 2 && event.cancelable) {
      event.preventDefault();
    }
  };

  vv.addEventListener("resize", onViewportResize);
  vv.addEventListener("scroll", onViewportResize);
  document.addEventListener("gesturestart", onGesture);
  document.addEventListener("gesturechange", onGesture);
  document.addEventListener("gestureend", onGesture);
  document.addEventListener("touchstart", onTouchStartPinchGuard, { passive: false });
  document.addEventListener("touchmove", onTouchMovePinchGuard, { passive: false });

  return () => {
    vv.removeEventListener("resize", onViewportResize);
    vv.removeEventListener("scroll", onViewportResize);
    document.removeEventListener("gesturestart", onGesture);
    document.removeEventListener("gesturechange", onGesture);
    document.removeEventListener("gestureend", onGesture);
    document.removeEventListener("touchstart", onTouchStartPinchGuard);
    document.removeEventListener("touchmove", onTouchMovePinchGuard);
    root.classList.remove(KEYBOARD_OPEN_CLASS);
    root.style.removeProperty(APP_HEIGHT_VAR);
    root.style.removeProperty(APP_TOP_VAR);
  };
}
