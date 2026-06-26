// Svelte action bridging touch events to the pure pull-to-refresh state. Kept
// thin: all decision logic lives in ./pullToRefresh so it stays testable.

import { pullStart, pullMove, pullEnd, type PullState } from "./pullToRefresh";

export interface PullOptions {
  onRefresh: () => void;
  onDistance?: (distance: number) => void;
  /** Scroll position of the relevant container; defaults to the document. */
  scrollTop?: () => number;
}

function readTouchY(event: Event): number | null {
  const touches = (event as TouchEvent).touches;
  if (!touches || touches.length !== 1) return null;
  return touches[0].clientY;
}

export function pullToRefresh(node: HTMLElement, options: PullOptions) {
  let opts = options;
  let state: PullState | null = null;
  let startY = 0;

  const scrollTop = () =>
    opts.scrollTop?.() ?? document.scrollingElement?.scrollTop ?? 0;

  const onStart = (event: Event) => {
    const y = readTouchY(event);
    if (y === null) return;
    startY = y;
    state = pullStart(scrollTop());
  };

  const onMove = (event: Event) => {
    if (!state?.active) return;
    const y = readTouchY(event);
    if (y === null) return;
    state = pullMove(state, y - startY);
    opts.onDistance?.(state.distance);
  };

  const onEnd = () => {
    if (state && pullEnd(state).triggered) opts.onRefresh();
    state = null;
    opts.onDistance?.(0);
  };

  node.addEventListener("touchstart", onStart, { passive: true });
  node.addEventListener("touchmove", onMove, { passive: true });
  node.addEventListener("touchend", onEnd);
  node.addEventListener("touchcancel", onEnd);

  return {
    update(next: PullOptions) {
      opts = next;
    },
    destroy() {
      node.removeEventListener("touchstart", onStart);
      node.removeEventListener("touchmove", onMove);
      node.removeEventListener("touchend", onEnd);
      node.removeEventListener("touchcancel", onEnd);
    },
  };
}
