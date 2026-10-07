import { SESSION_VIEWPORT_ATTR } from "@/shared/lib/sessionViewport";


const KEYBOARD_OPEN_DELTA_PX = 150;
const KEYBOARD_CLOSE_DELTA_PX = 100;
const KEYBOARD_CLOSE_SETTLE_MS = 250;
const KEYBOARD_OPEN_CLASS = "keyboard-open";
const APP_HEIGHT_VAR = "--app-height";
const APP_TOP_VAR = "--app-top";
const MIN_USABLE_HEIGHT_PX = 50;

function isUsableHeight(height: number): boolean {
  return height >= MIN_USABLE_HEIGHT_PX;
}

export function isKeyboardOpen(): boolean {
  return (
    typeof document !== "undefined" &&
    document.documentElement.classList.contains(KEYBOARD_OPEN_CLASS)
  );
}

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
  }
  document.documentElement.scrollTop = 0;
  document.body.scrollTop = 0;
  const scroller = document.scrollingElement;
  if (scroller) scroller.scrollTop = 0;
  for (const el of document.querySelectorAll<HTMLElement>('[data-testid="route-scroll"]')) {
    el.scrollTop = 0;
  }
}

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
  const clearAppGeometry = () => {
    root.style.removeProperty(APP_HEIGHT_VAR);
    root.style.removeProperty(APP_TOP_VAR);
  };

  const isSessionViewportOwned = () =>
    root.getAttribute(SESSION_VIEWPORT_ATTR) === "owned";

  const resolveViewportHeight = (): number | null => {
    const layoutHeight = window.innerHeight;
    if (isUsableHeight(vv.height)) {
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

  const dismissKeyboardOpen = () => {
    if (!keyboardOpen) return;
    keyboardOpen = false;
    root.classList.remove(KEYBOARD_OPEN_CLASS);
    blurSessionComposerIfFocused();
    resetDocumentScroll();
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

    if (!isUsableHeight(current)) {
      syncViewportGeometry();
      if (!keyboardOpen) {
        rebaseBaselineFromResolved();
      }
      return;
    }

    if (currentWidth !== baselineWidth) {
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
      root.classList.add(KEYBOARD_OPEN_CLASS);
      resetDocumentScroll();
    } else if (delta < KEYBOARD_CLOSE_DELTA_PX && keyboardOpen) {
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
      cancelCloseSettle();
    }
    if (!isUsableHeight(baselineHeight) && isUsableHeight(current)) {
      cancelCloseSettle();
      dismissKeyboardOpen();
      syncViewportGeometry();
      rebaseBaselineFromResolved();
      return;
    }

    syncViewportGeometry();
    if (!keyboardOpen) {
      rebaseBaselineFromResolved();
    }
  };

  const onGesture = (event: Event) => event.preventDefault();

  const onTouchMovePinchGuard = (event: TouchEvent) => {
    const scale = (event as TouchEvent & { scale?: number }).scale;
    if (typeof scale === "number" && scale !== 1) {
      event.preventDefault();
    }
  };

  const onTouchStartPinchGuard = (event: TouchEvent) => {
    if (event.touches && event.touches.length >= 2 && event.cancelable) {
      event.preventDefault();
    }
  };

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
    vv.removeEventListener("resize", onViewportResize);
    vv.removeEventListener("scroll", onViewportResize);
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
