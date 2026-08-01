import { useEffect, useRef, useState, type CSSProperties, type RefObject } from "react";
import {
  navigateSwipeCommitOffset,
  navigateSwipeEnd,
  navigateSwipeMove,
  navigateSwipeStart,
  navigateSwipeTranslateX,
  type NavigateSwipeState,
} from "@/shared/gestures/navigateSwipe";
import { setSwipeEnterDirection, type SwipeEnterDirection } from "@/shared/lib/swipeEnter";

export const SWIPE_PAGE_COMMIT_MS = 220;

export interface SwipePageTransitionOptions {
  onLeft?: () => void;
  onRight?: () => void;
  shouldIgnoreTarget?: (target: EventTarget | null) => boolean;
  /** Capture-phase listeners (task detail over terminal). Default true. */
  capture?: boolean;
}

export interface SwipePageTransitionResult {
  dragX: number;
  swiping: boolean;
  style: CSSProperties;
}

function readTouch(event: TouchEvent): { x: number; y: number } | null {
  const touch = event.changedTouches[0] ?? event.touches[0];
  if (!touch) return null;
  return { x: touch.clientX, y: touch.clientY };
}

export function useSwipePageTransition(
  ref: RefObject<HTMLElement | null>,
  options: SwipePageTransitionOptions,
): SwipePageTransitionResult {
  const optsRef = useRef(options);
  optsRef.current = options;
  const swipeRef = useRef<NavigateSwipeState>(navigateSwipeStart());
  const originRef = useRef({ x: 0, y: 0 });
  const touchTargetRef = useRef<EventTarget | null>(null);
  const [dragX, setDragX] = useState(0);
  const [dragging, setDragging] = useState(false);
  const [settling, setSettling] = useState(false);

  useEffect(() => {
    const root = ref.current;
    if (!root) return;

    const capture = options.capture ?? true;

    const pageWidth = () => root.clientWidth || window.innerWidth;

    const reset = () => {
      swipeRef.current = navigateSwipeStart();
      touchTargetRef.current = null;
      setDragX(0);
      setDragging(false);
      setSettling(false);
    };

    const animateTo = (targetX: number, direction: SwipeEnterDirection | null, then?: () => void) => {
      setDragging(false);
      setSettling(true);
      setDragX(targetX);

      let finished = false;
      const finish = () => {
        if (finished) return;
        finished = true;
        root.removeEventListener("transitionend", onTransitionEnd);
        window.clearTimeout(timer);
        if (direction) setSwipeEnterDirection(direction);
        if (then) {
          // Navigating unmounts this surface; skip reset to avoid a snap-back flash.
          then();
        } else {
          reset();
        }
      };

      const onTransitionEnd = (event: TransitionEvent) => {
        if (event.target !== root || event.propertyName !== "transform") return;
        finish();
      };

      const timer = window.setTimeout(finish, SWIPE_PAGE_COMMIT_MS + 40);
      root.addEventListener("transitionend", onTransitionEnd);
    };

    const onTouchStart = (event: TouchEvent) => {
      if (optsRef.current.shouldIgnoreTarget?.(event.target)) {
        touchTargetRef.current = null;
        return;
      }
      const point = readTouch(event);
      if (!point) return;
      touchTargetRef.current = event.target;
      originRef.current = point;
      swipeRef.current = navigateSwipeStart();
      setDragging(false);
      setSettling(false);
      setDragX(0);
    };

    const onTouchMove = (event: TouchEvent) => {
      if (!touchTargetRef.current) return;
      const point = readTouch(event);
      if (!point) return;
      const dx = point.x - originRef.current.x;
      const dy = point.y - originRef.current.y;
      const next = navigateSwipeMove(swipeRef.current, dx, dy, pageWidth());
      swipeRef.current = next;
      if (!next.engaged) return;
      if (event.cancelable) event.preventDefault();
      setDragging(true);
      setSettling(false);
      setDragX(navigateSwipeTranslateX(next));
    };

    const onTouchEnd = () => {
      if (!touchTargetRef.current) return;
      const direction = navigateSwipeEnd(swipeRef.current);
      const width = pageWidth();

      if (direction === "left" && optsRef.current.onLeft) {
        animateTo(
          navigateSwipeCommitOffset("left", width),
          "left",
          () => optsRef.current.onLeft?.(),
        );
        return;
      }
      if (direction === "right" && optsRef.current.onRight) {
        animateTo(
          navigateSwipeCommitOffset("right", width),
          "right",
          () => optsRef.current.onRight?.(),
        );
        return;
      }

      if (swipeRef.current.engaged) {
        animateTo(0, null);
        return;
      }
      reset();
    };

    root.addEventListener("touchstart", onTouchStart, { capture, passive: true });
    root.addEventListener("touchmove", onTouchMove, { capture, passive: false });
    root.addEventListener("touchend", onTouchEnd, { capture, passive: true });
    root.addEventListener("touchcancel", reset, { capture, passive: true });

    return () => {
      root.removeEventListener("touchstart", onTouchStart, capture);
      root.removeEventListener("touchmove", onTouchMove, capture);
      root.removeEventListener("touchend", onTouchEnd, capture);
      root.removeEventListener("touchcancel", reset, capture);
    };
  }, [options.capture, ref]);

  const swiping = dragging || settling;
  const style: CSSProperties = {
    transform: dragX ? `translate3d(${dragX}px, 0, 0)` : undefined,
    transition: dragging
      ? "none"
      : settling
        ? `transform ${SWIPE_PAGE_COMMIT_MS}ms var(--ease-spring)`
        : undefined,
  };

  return { dragX, swiping, style };
}
