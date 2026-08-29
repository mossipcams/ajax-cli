import {
  layoutViewportShrinksWithKeyboard,
  SESSION_VIEWPORT_ATTR,
} from "@/shared/lib/sessionViewport";

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
// iOS momentarily reports an expanded visualViewport mid-typing (keyboard
// morphs, autocorrect popovers). Tearing down the pinned band instantly for
// those transients is the "terminal jumps while typing" defect — the close
// edge only fires after the expansion persists for this window.
const KEYBOARD_CLOSE_SETTLE_MS = 250;
const KEYBOARD_OPEN_CLASS = "keyboard-open";
const APP_HEIGHT_VAR = "--app-height";
const APP_TOP_VAR = "--app-top";
// iOS Home Screen PWA splash can report visualViewport.height === 0; pinning
// that sets --app-height: 0px, so the CSS 100dvh fallback never applies and
// max-height: 0px clips the shell (#850).
const MIN_USABLE_HEIGHT_PX = 50;

function isUsableHeight(height: number): boolean {
  return height >= MIN_USABLE_HEIGHT_PX;
}

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
 * keyboard or expand snaps, including the App `[data-testid="route-scroll"]`
 * container that owns task-page vertical scroll. Safe in jsdom where
 * `scrollTo` is unimplemented.
 */
/** Blur the session composer when it owns focus — not the task terminal. */
export function blurSessionComposerIfFocused(): void {
  if (typeof document === "undefined") return;
  const composer = document.querySelector<HTMLTextAreaElement>(
    '[data-testid="session-composer"] textarea',
  );
  if (composer && document.activeElement === composer) {
    composer.blur();
  }
}

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
  for (const el of document.querySelectorAll<HTMLElement>('[data-testid="route-scroll"]')) {
    el.scrollTop = 0;
  }
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
  /** True when this keyboard session shrunk innerHeight with vv (PWA / iOS 26). */
  let layoutShrinksWithKeyboard = false;
  /** True while pointer/touch is down inside the session composer (#1113). */
  let sessionComposerPointerDown = false;

  const setAppHeight = (height: number) => {
    root.style.setProperty(APP_HEIGHT_VAR, `${height}px`);
  };
  const setAppTop = (offsetTop: number) => {
    root.style.setProperty(APP_TOP_VAR, `${offsetTop}px`);
  };
  const clearAppGeometry = () => {
    root.style.removeProperty(APP_HEIGHT_VAR);
    root.style.removeProperty(APP_TOP_VAR);
  };

  const isSessionViewportOwned = () =>
    root.getAttribute(SESSION_VIEWPORT_ATTR) === "owned";

  const resolveViewportHeight = (): number | null => {
    const layoutHeight = window.innerHeight;
    if (isUsableHeight(vv.height)) {
      // Session chat fills layout/dvh when the keyboard is closed; never pin to a
      // short visualViewport (tap-dismiss stale band or iOS ~24–34px discrepancy).
      if (!keyboardOpen && isSessionViewportOwned()) {
        return null;
      }
      return vv.height;
    }
    if (isUsableHeight(layoutHeight)) return layoutHeight;
    return null;
  };

  const restoreGeometryAfterKeyboardDismiss = () => {
    const layoutHeight = window.innerHeight;
    const visualHeight = vv.height;
    if (isSessionViewportOwned()) {
      clearAppGeometry();
      baselineHeight =
        layoutHeight - visualHeight > KEYBOARD_CLOSE_DELTA_PX
          ? layoutHeight
          : visualHeight;
    } else if (layoutHeight - visualHeight > KEYBOARD_CLOSE_DELTA_PX) {
      setAppHeight(layoutHeight);
      setAppTop(0);
      baselineHeight = layoutHeight;
    } else {
      syncViewportGeometry();
      baselineHeight = visualHeight;
    }
    baselineWidth = window.innerWidth;
  };

  const resolveViewportTop = (): number => {
    if (isUsableHeight(vv.height)) return vv.offsetTop ?? 0;
    return 0;
  };

  const syncViewportGeometry = () => {
    const height = resolveViewportHeight();
    if (height === null) {
      clearAppGeometry();
      return;
    }
    setAppHeight(height);
    setAppTop(resolveViewportTop());
  };

  const rebaseBaselineFromResolved = () => {
    const resolved = resolveViewportHeight();
    baselineHeight = resolved ?? vv.height;
    baselineWidth = window.innerWidth;
  };

  rebaseBaselineFromResolved();
  syncViewportGeometry();

  let closeSettleTimer: ReturnType<typeof setTimeout> | undefined;
  const cancelCloseSettle = () => {
    if (closeSettleTimer !== undefined) {
      clearTimeout(closeSettleTimer);
      closeSettleTimer = undefined;
    }
  };

  /** Deferred stale-viewport dismiss after composer pointerup (#1113). */
  let composerDismissTimer: ReturnType<typeof setTimeout> | undefined;
  const cancelComposerDismiss = () => {
    if (composerDismissTimer !== undefined) {
      clearTimeout(composerDismissTimer);
      composerDismissTimer = undefined;
    }
  };

  const dismissKeyboardOpen = () => {
    if (!keyboardOpen) return;
    keyboardOpen = false;
    layoutShrinksWithKeyboard = false;
    root.classList.remove(KEYBOARD_OPEN_CLASS);
    blurSessionComposerIfFocused();
    resetDocumentScroll();
  };

  const isInsideSessionComposer = (target: EventTarget | null): boolean => {
    if (!(target instanceof Element)) return false;
    return target.closest('[data-testid="session-composer"]') !== null;
  };

  const onSessionComposerPointerDown = (event: Event) => {
    if (isInsideSessionComposer(event.target)) {
      sessionComposerPointerDown = true;
    }
  };

  const dismissStaleVisualViewportAfterComposerGesture = () => {
    const layoutHeight = window.innerHeight;
    const visualHeight = vv.height;
    if (
      !keyboardOpen ||
      !layoutShrinksWithKeyboard ||
      sessionComposerPointerDown ||
      visualHeight < MIN_USABLE_HEIGHT_PX ||
      layoutHeight - visualHeight <= KEYBOARD_CLOSE_DELTA_PX
    ) {
      return false;
    }
    cancelCloseSettle();
    dismissKeyboardOpen();
    restoreGeometryAfterKeyboardDismiss();
    return true;
  };

  const onSessionComposerPointerUp = () => {
    sessionComposerPointerDown = false;
    // iOS synthetic click fires after pointerup; defer blur/relayout so Send/Attach
    // can land (#1113).
    cancelComposerDismiss();
    composerDismissTimer = setTimeout(() => {
      composerDismissTimer = undefined;
      dismissStaleVisualViewportAfterComposerGesture();
    }, 0);
  };

  const isFormControlFocused = () => {
    const active = document.activeElement;
    if (!active) return false;
    const tag = active.tagName;
    return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
  };

  const onViewportResize = () => {
    const current = vv.height;
    const currentWidth = window.innerWidth;

    // Splash can keep reporting height 0 after init already fell back to layout
    // height; a shrink to unusable must not look like a keyboard opening (#850).
    if (!isUsableHeight(current)) {
      syncViewportGeometry();
      if (!keyboardOpen) {
        rebaseBaselineFromResolved();
      }
      return;
    }

    if (currentWidth !== baselineWidth) {
      // Rotation: a real geometry change, close immediately.
      cancelCloseSettle();
      dismissKeyboardOpen();
      setAppHeight(current);
      setAppTop(resolveViewportTop());
      baselineHeight = current;
      baselineWidth = currentWidth;
      return;
    }
    const delta = baselineHeight - current;
    if (delta > KEYBOARD_OPEN_DELTA_PX && !keyboardOpen) {
      if (isSessionViewportOwned() && !isFormControlFocused()) {
        restoreGeometryAfterKeyboardDismiss();
        return;
      }
      cancelCloseSettle();
      keyboardOpen = true;
      layoutShrinksWithKeyboard = layoutViewportShrinksWithKeyboard(
        window.innerHeight,
        current,
      );
      root.classList.add(KEYBOARD_OPEN_CLASS);
      resetDocumentScroll();
    } else if (delta < KEYBOARD_CLOSE_DELTA_PX && keyboardOpen) {
      // Hold the pinned band (class AND geometry) until the expansion proves
      // it is a real keyboard dismissal, not a mid-typing transient.
      if (closeSettleTimer === undefined) {
        closeSettleTimer = setTimeout(() => {
          closeSettleTimer = undefined;
          if (!keyboardOpen) return;
          const settledDelta = baselineHeight - vv.height;
          if (settledDelta < KEYBOARD_CLOSE_DELTA_PX) {
            dismissKeyboardOpen();
            restoreGeometryAfterKeyboardDismiss();
          }
        }, KEYBOARD_CLOSE_SETTLE_MS);
      }
      return;
    } else if (keyboardOpen && closeSettleTimer !== undefined) {
      // Shrank back under the close threshold: the expansion was a transient.
      cancelCloseSettle();
    }
    // Splash recovery: visualViewport can jump from 0/unusable to full height.
    // Rebase without treating the expansion as keyboard dismissal/opening.
    if (!isUsableHeight(baselineHeight) && isUsableHeight(current)) {
      cancelCloseSettle();
      dismissKeyboardOpen();
      syncViewportGeometry();
      rebaseBaselineFromResolved();
      return;
    }

    // Keep --app-height pinned to the visible band. While the keyboard is closed
    // this also tracks address-bar / orientation changes and re-bases the
    // threshold so the next keyboard open is measured from the right height.
    syncViewportGeometry();
    if (!keyboardOpen) {
      rebaseBaselineFromResolved();
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

  // iOS dismisses the soft keyboard on app-switch; if keyboard-open stays
  // latched, CSS hides .bottom-nav and pins the band short (#836).
  const onForegroundResync = () => {
    cancelCloseSettle();
    const wasKeyboardOpen = keyboardOpen;
    dismissKeyboardOpen();
    restoreGeometryAfterKeyboardDismiss();
    if (!wasKeyboardOpen) {
      resetDocumentScroll();
    }
  };

  const onVisibilityChange = () => {
    if (document.visibilityState === "hidden") {
      // Keyboard is gone once backgrounded; do not wait for visualViewport.
      cancelCloseSettle();
      dismissKeyboardOpen();
      return;
    }
    if (document.visibilityState === "visible") {
      onForegroundResync();
    }
  };

  const onPageShow = () => {
    onForegroundResync();
  };

  const onSessionComposerFocusOut = () => {
    requestAnimationFrame(() => {
      if (isFormControlFocused()) return;
      if (root.getAttribute(SESSION_VIEWPORT_ATTR) !== "owned") return;
      cancelCloseSettle();
      dismissKeyboardOpen();
      restoreGeometryAfterKeyboardDismiss();
    });
  };

  vv.addEventListener("resize", onViewportResize);
  vv.addEventListener("scroll", onViewportResize);
  document.addEventListener("pointerdown", onSessionComposerPointerDown, true);
  document.addEventListener("pointerup", onSessionComposerPointerUp, true);
  document.addEventListener("pointercancel", onSessionComposerPointerUp, true);
  document.addEventListener("touchstart", onSessionComposerPointerDown, true);
  document.addEventListener("touchend", onSessionComposerPointerUp, true);
  document.addEventListener("touchcancel", onSessionComposerPointerUp, true);
  document.addEventListener("focusout", onSessionComposerFocusOut, true);
  document.addEventListener("visibilitychange", onVisibilityChange);
  window.addEventListener("pageshow", onPageShow);
  document.addEventListener("gesturestart", onGesture);
  document.addEventListener("gesturechange", onGesture);
  document.addEventListener("gestureend", onGesture);
  document.addEventListener("touchstart", onTouchStartPinchGuard, { passive: false });
  document.addEventListener("touchmove", onTouchMovePinchGuard, { passive: false });

  return () => {
    cancelCloseSettle();
    cancelComposerDismiss();
    vv.removeEventListener("resize", onViewportResize);
    vv.removeEventListener("scroll", onViewportResize);
    document.removeEventListener("pointerdown", onSessionComposerPointerDown, true);
    document.removeEventListener("pointerup", onSessionComposerPointerUp, true);
    document.removeEventListener("pointercancel", onSessionComposerPointerUp, true);
    document.removeEventListener("touchstart", onSessionComposerPointerDown, true);
    document.removeEventListener("touchend", onSessionComposerPointerUp, true);
    document.removeEventListener("touchcancel", onSessionComposerPointerUp, true);
    document.removeEventListener("focusout", onSessionComposerFocusOut, true);
    document.removeEventListener("visibilitychange", onVisibilityChange);
    window.removeEventListener("pageshow", onPageShow);
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
