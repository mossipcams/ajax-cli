import { useEffect, useRef, type RefObject } from "react";
import { swipeStart, swipeMove, swipeEnd, type SwipeState } from "@/shared/gestures/swipeReveal";
import { gestureBusyGate } from "@/shared/lib/cockpitPoll";

export interface SwipeOptions {
  onOffset?: (offset: number) => void;
  onOpenChange?: (open: boolean) => void;
  getInitialOffset?: () => number;
  ignoreSelector?: string;
  onDragChange?: (dragging: boolean) => void;
  target?: RefObject<HTMLElement | null>;
}

function readTouch(event: Event): { x: number; y: number } | null {
  const touches = (event as TouchEvent).touches;
  if (!touches || touches.length !== 1) return null;
  return { x: touches[0].clientX, y: touches[0].clientY };
}

function translate(offset: number): string {
  return `translateX(-${offset}px)`;
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
    let touchHeld = false;
    let frame: number | null = null;
    let frameOffset = 0;

    const writeOffset = (offset: number) => {
      const target = optsRef.current.target?.current;
      if (target) target.style.transform = translate(offset);
    };

    const cancelFrame = () => {
      if (frame === null) return;
      cancelAnimationFrame(frame);
      frame = null;
    };

    const scheduleOffset = (offset: number) => {
      frameOffset = offset;
      if (frame !== null) return;
      frame = requestAnimationFrame(() => {
        frame = null;
        writeOffset(frameOffset);
      });
    };

    const holdTouch = () => {
      if (!touchHeld) {
        touchHeld = true;
        optsRef.current.onDragChange?.(true);
        gestureBusyGate.begin();
      }
    };

    const releaseTouch = () => {
      if (touchHeld) {
        touchHeld = false;
        optsRef.current.onDragChange?.(false);
        gestureBusyGate.end();
      }
    };

    const onStart = (event: Event) => {
      const ignoreSelector = optsRef.current.ignoreSelector;
      if (
        ignoreSelector &&
        event.target instanceof Element &&
        event.target.closest(ignoreSelector)
      ) {
        return;
      }
      const point = readTouch(event);
      if (!point) return;
      startX = point.x;
      startY = point.y;
      state = swipeStart(optsRef.current.getInitialOffset?.() ?? 0);
      holdTouch();
    };

    const onMove = (event: Event) => {
      if (!state) return;
      const point = readTouch(event);
      if (!point) return;
      state = swipeMove(state, point.x - startX, point.y - startY);
      if (!state.engaged) return;
      if (optsRef.current.target && event.cancelable) event.preventDefault();
      scheduleOffset(state.offset);
    };

    const onEnd = () => {
      if (!state) {
        releaseTouch();
        return;
      }
      cancelFrame();
      const settled = swipeEnd(state);
      writeOffset(settled.offset);
      optsRef.current.onOffset?.(settled.offset);
      optsRef.current.onOpenChange?.(settled.open);
      state = null;
      releaseTouch();
    };

    const onCancel = () => {
      if (!state) {
        releaseTouch();
        return;
      }
      cancelFrame();
      writeOffset(state.baseOffset);
      state = null;
      releaseTouch();
    };

    node.addEventListener("touchstart", onStart, { passive: true });
    node.addEventListener("touchmove", onMove, { passive: false });
    node.addEventListener("touchend", onEnd);
    node.addEventListener("touchcancel", onCancel);

    return () => {
      cancelFrame();
      releaseTouch();
      node.removeEventListener("touchstart", onStart);
      node.removeEventListener("touchmove", onMove);
      node.removeEventListener("touchend", onEnd);
      node.removeEventListener("touchcancel", onCancel);
    };
  }, [ref]);
}
