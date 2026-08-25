/** Session chat visible-height math — one authoritative band for orchestration chat. */

export const SESSION_KEYBOARD_OPEN_PX = 100;
export const MIN_USABLE_VIEWPORT_PX = 50;
export const SESSION_PIN_THRESHOLD_PX = 48;
export const SESSION_VIEWPORT_ATTR = "data-session-viewport";
export const LAYOUT_STABLE_FRAMES = 2;
export const LAYOUT_POLL_MAX_FRAMES = 20;

function layoutKey(node: HTMLDivElement): string {
  return `${node.scrollHeight}:${node.clientHeight}`;
}

/** Transcript scroll snapshot captured before a keyboard or layout transition. */
export interface TranscriptGeometry {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  atBottom: boolean;
}

export function transcriptAtBottom(
  scrollTop: number,
  scrollHeight: number,
  clientHeight: number,
  threshold = SESSION_PIN_THRESHOLD_PX,
): boolean {
  return scrollHeight - scrollTop - clientHeight < threshold;
}

export function captureTranscriptGeometry(node: HTMLDivElement): TranscriptGeometry {
  const { scrollTop, scrollHeight, clientHeight } = node;
  return {
    scrollTop,
    scrollHeight,
    clientHeight,
    atBottom: transcriptAtBottom(scrollTop, scrollHeight, clientHeight),
  };
}

/**
 * Restore equivalent transcript position after layout settles. No animation.
 * At-bottom → new live edge; history → same visible content plus any
 * scrollHeight growth above the viewport.
 */
export function restoreTranscriptGeometry(
  node: HTMLDivElement,
  before: TranscriptGeometry,
): void {
  if (before.atBottom) {
    node.scrollTop = node.scrollHeight;
    return;
  }
  const heightDelta = node.scrollHeight - before.scrollHeight;
  node.scrollTop = before.scrollTop + heightDelta;
}

/** Poll until scrollHeight/clientHeight stop changing, then run restore once. */
export function afterTranscriptLayoutSettles(
  node: HTMLDivElement,
  restoreTarget: TranscriptGeometry,
  restore: () => void,
  options?: { ignoreProgrammaticScroll?: { current: boolean } },
): () => void {
  const ignore = options?.ignoreProgrammaticScroll;
  const scrollProgrammatically = (top: number) => {
    if (ignore) ignore.current = true;
    node.scrollTop = top;
    if (ignore) ignore.current = false;
  };

  let raf = 0;
  let stableFrames = 0;
  let lastKey = layoutKey(node);
  let frameCount = 0;

  const poll = () => {
    frameCount++;
    if (restoreTarget.atBottom) {
      scrollProgrammatically(node.scrollHeight);
    }
    const key = layoutKey(node);
    if (key === lastKey) {
      stableFrames++;
    } else {
      stableFrames = 0;
      lastKey = key;
    }

    if (stableFrames >= LAYOUT_STABLE_FRAMES || frameCount >= LAYOUT_POLL_MAX_FRAMES) {
      restore();
      return;
    }
    raf = requestAnimationFrame(poll);
  };

  raf = requestAnimationFrame(poll);
  return () => cancelAnimationFrame(raf);
}

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
