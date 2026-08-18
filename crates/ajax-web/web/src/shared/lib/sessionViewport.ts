/** Session chat visible-height math — one authoritative band for orchestration chat. */

export const SESSION_KEYBOARD_OPEN_PX = 100;
export const MIN_USABLE_VIEWPORT_PX = 50;
export const SESSION_PIN_THRESHOLD_PX = 48;
export const SESSION_VIEWPORT_ATTR = "data-session-viewport";

/** Tell global keyboard CSS that session chat owns its band geometry. */
export function claimSessionViewportOwnership(): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute(SESSION_VIEWPORT_ATTR, "owned");
}

export function releaseSessionViewportOwnership(): void {
  if (typeof document === "undefined") return;
  document.documentElement.removeAttribute(SESSION_VIEWPORT_ATTR);
}

/** Layout viewport already shrank with the keyboard (PWA / Android / iOS 26). */
export function layoutViewportShrinksWithKeyboard(
  innerHeight: number,
  visualViewportHeight: number,
): boolean {
  return innerHeight - visualViewportHeight < 50;
}

/** True when visualViewport occlusion looks like a soft keyboard, not URL-bar drift. */
export function isSessionKeyboardOpen(
  fullHeight: number,
  visualViewportHeight: number,
): boolean {
  if (visualViewportHeight < MIN_USABLE_VIEWPORT_PX) return false;
  return fullHeight - visualViewportHeight > SESSION_KEYBOARD_OPEN_PX;
}

/**
 * Bottom padding for iOS regular Safari where innerHeight stays full while
 * visualViewport shrinks. Zero when the layout viewport already accounts for
 * the keyboard so flex / dvh is not padded twice.
 */
export function sessionKeyboardPadding(
  innerHeight: number,
  visualViewportHeight: number,
  keyboardOpen: boolean,
  safeBottomPx = 0,
): number {
  if (!keyboardOpen) return 0;
  if (layoutViewportShrinksWithKeyboard(innerHeight, visualViewportHeight)) return 0;
  return Math.max(0, innerHeight - visualViewportHeight - safeBottomPx);
}

/** Visible band height for the session surface (visualViewport when usable). */
export function sessionVisibleHeight(
  innerHeight: number,
  visualViewportHeight: number,
): number {
  if (visualViewportHeight >= MIN_USABLE_VIEWPORT_PX) return visualViewportHeight;
  return innerHeight;
}

/**
 * Inline style for the bounded session flex column. Reserves keyboard height
 * only on iOS regular Safari; returns undefined elsewhere so dvh/flex owns it.
 */
export function sessionSurfaceStyle(
  innerHeight: number,
  visualViewportHeight: number,
  keyboardOpen: boolean,
  safeBottomPx = 0,
): { paddingBottom: number } | undefined {
  const paddingBottom = sessionKeyboardPadding(
    innerHeight,
    visualViewportHeight,
    keyboardOpen,
    safeBottomPx,
  );
  return paddingBottom > 0 ? { paddingBottom } : undefined;
}
