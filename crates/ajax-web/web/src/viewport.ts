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
 * Begin syncing `--app-height` / `keyboard-open` from `visualViewport`.
 * No-ops where `visualViewport` is unavailable. Returns a cleanup function
 * that removes every listener and the state it set.
 */
export function initViewport(): () => void {
  const vv = typeof window !== "undefined" ? window.visualViewport : undefined;
  if (!vv) return () => {};

  const root = document.documentElement;
  let baselineHeight = vv.height;
  let keyboardOpen = false;

  const setAppHeight = (height: number) => {
    root.style.setProperty("--app-height", `${height}px`);
  };
  setAppHeight(vv.height);

  const onViewportResize = () => {
    const current = vv.height;
    const delta = baselineHeight - current;
    if (delta > KEYBOARD_OPEN_DELTA_PX && !keyboardOpen) {
      keyboardOpen = true;
      root.classList.add(KEYBOARD_OPEN_CLASS);
    } else if (delta < KEYBOARD_CLOSE_DELTA_PX && keyboardOpen) {
      keyboardOpen = false;
      root.classList.remove(KEYBOARD_OPEN_CLASS);
    }
    // Keep --app-height pinned to the visible band. While the keyboard is closed
    // this also tracks address-bar / orientation changes and re-bases the
    // threshold so the next keyboard open is measured from the right height.
    setAppHeight(current);
    if (!keyboardOpen) baselineHeight = current;
  };

  // iOS scrolls the document to chase xterm's hidden textarea when the keyboard
  // opens; snap it back so the fixed terminal layer never slides off-screen.
  const onWindowScroll = () => {
    if (keyboardOpen) window.scrollTo(0, 0);
  };

  // Suppress pinch / double-tap zoom (iOS ignores user-scalable=no since iOS 10).
  const onGesture = (event: Event) => event.preventDefault();

  vv.addEventListener("resize", onViewportResize);
  vv.addEventListener("scroll", onViewportResize);
  window.addEventListener("scroll", onWindowScroll);
  document.addEventListener("gesturestart", onGesture);
  document.addEventListener("gesturechange", onGesture);

  return () => {
    vv.removeEventListener("resize", onViewportResize);
    vv.removeEventListener("scroll", onViewportResize);
    window.removeEventListener("scroll", onWindowScroll);
    document.removeEventListener("gesturestart", onGesture);
    document.removeEventListener("gesturechange", onGesture);
    root.classList.remove(KEYBOARD_OPEN_CLASS);
    root.style.removeProperty("--app-height");
  };
}
