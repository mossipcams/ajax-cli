export const SESSION_KEYBOARD_OPEN_PX = 100;
export const MIN_USABLE_VIEWPORT_PX = 50;
export const SESSION_PIN_THRESHOLD_PX = 16;
export const SESSION_VIEWPORT_ATTR = "data-session-viewport";
export const LAYOUT_STABLE_FRAMES = 2;
export const LAYOUT_POLL_MAX_FRAMES = 20;

const THREAD_INNER_SELECTOR = ":scope > .session-thread-inner";

function layoutKey(node: HTMLDivElement): string {
  return `${node.scrollHeight}:${node.clientHeight}`;
}

export function getThreadInner(thread: HTMLDivElement): HTMLElement {
  return thread.querySelector<HTMLElement>(THREAD_INNER_SELECTOR) ?? thread;
}

export function findLiveEdgeAnchor(thread: HTMLDivElement): HTMLElement | null {
  const inner = getThreadInner(thread);
  for (let i = inner.children.length - 1; i >= 0; i -= 1) {
    const child = inner.children[i];
    if (child instanceof HTMLElement) return child;
  }
  return null;
}

export function transcriptScrollBottom(
  scrollTop: number,
  scrollHeight: number,
  clientHeight: number,
): number {
  return scrollHeight - scrollTop - clientHeight;
}

export function transcriptAtBottom(
  scrollTop: number,
  scrollHeight: number,
  clientHeight: number,
  threshold = SESSION_PIN_THRESHOLD_PX,
): boolean {
  return transcriptScrollBottom(scrollTop, scrollHeight, clientHeight) <= threshold;
}

export function transcriptAtLiveEdge(
  thread: HTMLDivElement,
  threshold = SESSION_PIN_THRESHOLD_PX,
): boolean {
  return transcriptAtBottom(thread.scrollTop, thread.scrollHeight, thread.clientHeight, threshold);
}

export function pinTranscriptToLiveEdge(thread: HTMLDivElement): void {
  thread.scrollTop = Math.max(0, thread.scrollHeight - thread.clientHeight);
}

export interface TranscriptGeometry {
  atBottom: boolean;
  scrollTop?: number;
  scrollHeight?: number;
}

export function captureTranscriptGeometry(node: HTMLDivElement): TranscriptGeometry {
  const atBottom = transcriptAtLiveEdge(node);
  if (atBottom) return { atBottom: true };
  return {
    atBottom: false,
    scrollTop: node.scrollTop,
    scrollHeight: node.scrollHeight,
  };
}

export function restoreTranscriptGeometry(
  node: HTMLDivElement,
  before: TranscriptGeometry,
): void {
  if (before.atBottom) {
    pinTranscriptToLiveEdge(node);
    return;
  }
  if (before.scrollTop === undefined || before.scrollHeight === undefined) return;
  const heightDelta = node.scrollHeight - before.scrollHeight;
  node.scrollTop = before.scrollTop + heightDelta;
}

export function afterTranscriptLayoutSettles(
  node: HTMLDivElement,
  restoreTarget: TranscriptGeometry,
  restore: () => void,
  options?: {
    ignoreProgrammaticScroll?: { current: boolean };
    pinnedRef?: { current: boolean };
  },
): () => void {
  const ignore = options?.ignoreProgrammaticScroll;
  const pinnedRef = options?.pinnedRef;

  const scrollProgrammatically = (fn: () => void) => {
    if (ignore) ignore.current = true;
    fn();
    if (ignore) ignore.current = false;
  };

  let raf = 0;
  let stableFrames = 0;
  let lastKey = layoutKey(node);
  let frameCount = 0;

  const poll = () => {
    frameCount++;
    if (restoreTarget.atBottom && (!pinnedRef || pinnedRef.current)) {
      scrollProgrammatically(() => pinTranscriptToLiveEdge(node));
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

export function claimSessionViewportOwnership(): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute(SESSION_VIEWPORT_ATTR, "owned");
}

export function releaseSessionViewportOwnership(): void {
  if (typeof document === "undefined") return;
  document.documentElement.removeAttribute(SESSION_VIEWPORT_ATTR);
}

export function layoutViewportShrinksWithKeyboard(
  innerHeight: number,
  visualViewportHeight: number,
): boolean {
  return innerHeight - visualViewportHeight < 50;
}

export function isSessionKeyboardOpen(
  fullHeight: number,
  visualViewportHeight: number,
): boolean {
  if (visualViewportHeight < MIN_USABLE_VIEWPORT_PX) return false;
  return fullHeight - visualViewportHeight > SESSION_KEYBOARD_OPEN_PX;
}

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

export function sessionVisibleHeight(
  innerHeight: number,
  visualViewportHeight: number,
): number {
  if (visualViewportHeight >= MIN_USABLE_VIEWPORT_PX) return visualViewportHeight;
  return innerHeight;
}

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
