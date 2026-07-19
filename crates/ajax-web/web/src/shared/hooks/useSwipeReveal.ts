import { useEffect, useRef, type RefObject } from "react";
import { swipeStart, swipeMove, swipeEnd, type SwipeState } from "@/shared/gestures/swipeReveal";

export interface SwipeOptions {
  onOffset?: (offset: number) => void;
  onOpenChange?: (open: boolean) => void;
}

function readTouch(event: Event): { x: number; y: number } | null {
  const touches = (event as TouchEvent).touches;
  if (!touches || touches.length !== 1) return null;
  return { x: touches[0].clientX, y: touches[0].clientY };
}

export function useSwipeReveal(
  ref: RefObject<HTMLElement | null>,
  options: SwipeOptions,
): void {
  const optsRef = useRef(options);
  optsRef.current = options;

  useEffect(() => {
    const node = ref.current;
    if (!node) return;

    let state: SwipeState | null = null;
    let startX = 0;
    let startY = 0;

    const onStart = (event: Event) => {
      const point = readTouch(event);
      if (!point) return;
      startX = point.x;
      startY = point.y;
      state = swipeStart();
    };

    const onMove = (event: Event) => {
      if (!state) return;
      const point = readTouch(event);
      if (!point) return;
      state = swipeMove(state, point.x - startX, point.y - startY);
      if (state.engaged) optsRef.current.onOffset?.(state.offset);
    };

    const onEnd = () => {
      if (!state) return;
      const settled = swipeEnd(state);
      optsRef.current.onOffset?.(settled.offset);
      optsRef.current.onOpenChange?.(settled.open);
      state = null;
    };

    node.addEventListener("touchstart", onStart, { passive: true });
    node.addEventListener("touchmove", onMove, { passive: true });
    node.addEventListener("touchend", onEnd);
    node.addEventListener("touchcancel", onEnd);

    return () => {
      node.removeEventListener("touchstart", onStart);
      node.removeEventListener("touchmove", onMove);
      node.removeEventListener("touchend", onEnd);
      node.removeEventListener("touchcancel", onEnd);
    };
  }, [ref]);
}
