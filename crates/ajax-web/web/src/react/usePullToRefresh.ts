import { useCallback, useEffect, useRef } from "react";
import { pullStart, pullMove, pullEnd, type PullState } from "../gestures/pullToRefresh";

export interface PullToRefreshOptions {
  onRefresh: () => void;
  onDistance?: (distance: number) => void;
  scrollTop?: () => number;
}

function readTouchY(event: Event): number | null {
  const touches = (event as TouchEvent).touches;
  if (!touches || touches.length !== 1) return null;
  return touches[0].clientY;
}

export function usePullToRefresh(
  options: PullToRefreshOptions,
): (node: HTMLElement | null) => void {
  const optsRef = useRef(options);
  optsRef.current = options;
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      cleanupRef.current?.();
      cleanupRef.current = null;
    };
  }, []);

  return useCallback((node: HTMLElement | null) => {
    cleanupRef.current?.();
    cleanupRef.current = null;
    if (!node) return;

    let state: PullState | null = null;
    let startY = 0;

    const scrollTop = () =>
      optsRef.current.scrollTop?.() ?? document.scrollingElement?.scrollTop ?? 0;

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
      optsRef.current.onDistance?.(state.distance);
    };

    const onEnd = () => {
      if (state && pullEnd(state).triggered) optsRef.current.onRefresh();
      state = null;
      optsRef.current.onDistance?.(0);
    };

    node.addEventListener("touchstart", onStart, { passive: true });
    node.addEventListener("touchmove", onMove, { passive: true });
    node.addEventListener("touchend", onEnd);
    node.addEventListener("touchcancel", onEnd);

    cleanupRef.current = () => {
      node.removeEventListener("touchstart", onStart);
      node.removeEventListener("touchmove", onMove);
      node.removeEventListener("touchend", onEnd);
      node.removeEventListener("touchcancel", onEnd);
    };
  }, []);
}
