import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type RefObject,
} from "react";
import {
  navigateSwipeCommitOffset,
  navigateSwipeEnd,
  navigateSwipeMove,
  navigateSwipeStart,
  navigateSwipeTranslateX,
  type NavigateSwipeState,
} from "@/shared/gestures/navigateSwipe";
import { gestureBusyGate } from "@/shared/lib/cockpitPoll";
import { setSwipeEnterDirection, type SwipeEnterDirection } from "@/shared/lib/swipeEnter";
import { captureSwipe, markNavigationStart } from "@/shared/lib/telemetry";
import { shouldSuppressPageSwipe } from "@/shared/lib/terminalSelecting";
export const SWIPE_PAGE_COMMIT_MS = 220;

export interface SwipePageTransitionOptions {
  onLeft?: () => void;
  onRight?: () => void;
  shouldIgnoreTarget?: (target: EventTarget | null) => boolean;
  /** Capture-phase listeners (task detail over terminal). Default true. */
  capture?: boolean;
  from_route?: string;
  to_routeLeft?: string;
  to_routeRight?: string;
}

export type SwipePageCommitDirection = "left" | "right";

export interface SwipePageTransitionResult {
  dragX: number;
  swiping: boolean;
  style: CSSProperties;
  /** Programmatic commit — same exit+enter path as a successful swipe. */
  commit: (direction: SwipePageCommitDirection) => void;
}

function readTouch(event: TouchEvent): { x: number; y: number } | null {
  const touch = event.changedTouches[0] ?? event.touches[0];
  if (!touch) return null;
  return { x: touch.clientX, y: touch.clientY };
}

function swipeVelocity(distance_px: number, duration_ms: number): number {
  if (duration_ms <= 0) return 0;
  return Math.round((distance_px / duration_ms) * 1000) / 1000;
}

export function useSwipePageTransition(
  ref: RefObject<HTMLElement | null>,
  options: SwipePageTransitionOptions,
): SwipePageTransitionResult {
  const optsRef = useRef(options);
  optsRef.current = options;
  const swipeRef = useRef<NavigateSwipeState>(navigateSwipeStart());
  const originRef = useRef({ x: 0, y: 0 });
  const touchStartedAtRef = useRef(0);
  const touchTargetRef = useRef<EventTarget | null>(null);
  const maxDistanceRef = useRef(0);
  const [dragX, setDragX] = useState(0);
  const [dragging, setDragging] = useState(false);
  const [settling, setSettling] = useState(false);
  const settlingRef = useRef(false);
  const dragGestureHeldRef = useRef(false);
  const settleGestureHeldRef = useRef(false);
  const commitRef = useRef<(direction: SwipePageCommitDirection) => void>(() => {});

  const holdDragGesture = () => {
    if (!dragGestureHeldRef.current) {
      dragGestureHeldRef.current = true;
      gestureBusyGate.begin();
    }
  };

  const releaseDragGesture = () => {
    if (dragGestureHeldRef.current) {
      dragGestureHeldRef.current = false;
      gestureBusyGate.end();
    }
  };

  const holdSettleGesture = () => {
    if (!settleGestureHeldRef.current) {
      settleGestureHeldRef.current = true;
      gestureBusyGate.begin();
    }
  };

  const releaseSettleGesture = () => {
    if (settleGestureHeldRef.current) {
      settleGestureHeldRef.current = false;
      gestureBusyGate.end();
    }
  };

  const releaseGestures = () => {
    releaseDragGesture();
    releaseSettleGesture();
  };

  useEffect(() => {
    const root = ref.current;
    if (!root) return;

    const capture = options.capture ?? true;

    const pageWidth = () => root.clientWidth || window.innerWidth;

    const readFromRoute = () =>
      optsRef.current.from_route ?? window.location.hash;

    const reset = () => {
      swipeRef.current = navigateSwipeStart();
      touchTargetRef.current = null;
      settlingRef.current = false;
      maxDistanceRef.current = 0;
      releaseGestures();
      setDragX(0);
      setDragging(false);
      setSettling(false);
    };

    let settleTimer: ReturnType<typeof setTimeout> | null = null;
    let settleOnTransitionEnd: ((event: TransitionEvent) => void) | null = null;
    let settleAborted = false;

    const clearSettle = () => {
      if (settleOnTransitionEnd) {
        root.removeEventListener("transitionend", settleOnTransitionEnd);
        settleOnTransitionEnd = null;
      }
      if (settleTimer !== null) {
        window.clearTimeout(settleTimer);
        settleTimer = null;
      }
    };

    const abortSettle = () => {
      settleAborted = true;
      clearSettle();
    };

    const animateTo = (
      targetX: number,
      direction: SwipeEnterDirection | null,
      swipeOutcome: {
        direction: "left" | "right";
        duration_ms: number;
        distance_px: number;
        completed: boolean;
        cancelled: boolean;
        to_route?: string;
      } | null,
      then?: () => void,
    ) => {
      if (settlingRef.current) return;
      settlingRef.current = true;
      settleAborted = false;
      releaseDragGesture();
      holdSettleGesture();
      setDragging(false);
      setSettling(true);
      setDragX(targetX);
      const settleStartedAt = performance.now();
      const fromRouteAtCommit = readFromRoute();

      let finished = false;
      const finish = () => {
        if (finished || settleAborted) return;
        finished = true;
        clearSettle();
        const settle_ms = Math.round(performance.now() - settleStartedAt);
        if (swipeOutcome) {
          const page_width_px = pageWidth();
          captureSwipe({
            direction: swipeOutcome.direction,
            duration_ms: swipeOutcome.duration_ms,
            distance_px: swipeOutcome.distance_px,
            page_width_px,
            velocity_px_per_ms: swipeVelocity(
              swipeOutcome.distance_px,
              swipeOutcome.duration_ms,
            ),
            completed: swipeOutcome.completed,
            cancelled: swipeOutcome.cancelled,
            settle_ms,
            from_route: fromRouteAtCommit,
            ...(swipeOutcome.to_route ? { to_route: swipeOutcome.to_route } : {}),
          });
        }
        if (direction) setSwipeEnterDirection(direction);
        if (then) {
          const toRoute = swipeOutcome?.to_route;
          if (toRoute !== undefined && toRoute === window.location.hash) {
            reset();
            return;
          }
          markNavigationStart(fromRouteAtCommit, "swipe");
          // Navigating unmounts this surface; skip reset to avoid a snap-back flash.
          releaseSettleGesture();
          then();
        } else {
          reset();
        }
      };

      const onTransitionEnd = (event: TransitionEvent) => {
        if (event.target !== root || event.propertyName !== "transform") return;
        finish();
      };

      settleTimer = window.setTimeout(finish, SWIPE_PAGE_COMMIT_MS + 40);
      settleOnTransitionEnd = onTransitionEnd;
      root.addEventListener("transitionend", onTransitionEnd);
    };

    commitRef.current = (direction: SwipePageCommitDirection) => {
      const width = pageWidth();
      const swipeOutcome = {
        direction,
        duration_ms: 0,
        distance_px: width,
        completed: true,
        cancelled: false,
        to_route:
          direction === "left"
            ? optsRef.current.to_routeLeft
            : optsRef.current.to_routeRight,
      };
      if (direction === "left" && optsRef.current.onLeft) {
        animateTo(
          navigateSwipeCommitOffset("left", width),
          "left",
          swipeOutcome,
          () => optsRef.current.onLeft?.(),
        );
        return;
      }
      if (direction === "right" && optsRef.current.onRight) {
        animateTo(
          navigateSwipeCommitOffset("right", width),
          "right",
          swipeOutcome,
          () => optsRef.current.onRight?.(),
        );
      }
    };

    const onTouchStart = (event: TouchEvent) => {
      if (optsRef.current.shouldIgnoreTarget?.(event.target)) {
        touchTargetRef.current = null;
        return;
      }
      // Capture runs before terminal bubble: refuse to arm while selecting, or
      // while a double-tap is pending on the terminal (second contact).
      if (shouldSuppressPageSwipe(event.target)) {
        touchTargetRef.current = null;
        return;
      }
      const point = readTouch(event);
      if (!point) return;
      touchTargetRef.current = event.target;
      originRef.current = point;
      touchStartedAtRef.current = performance.now();
      maxDistanceRef.current = 0;
      swipeRef.current = navigateSwipeStart();
      setDragging(false);
      setSettling(false);
      setDragX(0);
    };

    const onTouchMove = (event: TouchEvent) => {
      if (!touchTargetRef.current) return;
      const target = event.target ?? touchTargetRef.current;
      if (
        shouldSuppressPageSwipe(target) ||
        optsRef.current.shouldIgnoreTarget?.(target)
      ) {
        reset();
        return;
      }
      const point = readTouch(event);
      if (!point) return;
      const dx = point.x - originRef.current.x;
      const dy = point.y - originRef.current.y;
      const next = navigateSwipeMove(swipeRef.current, dx, dy, pageWidth());
      swipeRef.current = next;
      if (!next.engaged) return;
      if (event.cancelable) event.preventDefault();
      const distance = Math.abs(navigateSwipeTranslateX(next));
      if (distance > maxDistanceRef.current) {
        maxDistanceRef.current = distance;
      }
      holdDragGesture();
      setDragging(true);
      setSettling(false);
      setDragX(navigateSwipeTranslateX(next));
    };

    const onTouchEnd = () => {
      if (!touchTargetRef.current) return;
      if (
        shouldSuppressPageSwipe(touchTargetRef.current) ||
        optsRef.current.shouldIgnoreTarget?.(touchTargetRef.current)
      ) {
        reset();
        return;
      }
      const direction = navigateSwipeEnd(swipeRef.current);
      const width = pageWidth();
      const duration_ms = Math.round(performance.now() - touchStartedAtRef.current);
      const distance_px = Math.round(maxDistanceRef.current);

      if (direction === "left" && optsRef.current.onLeft) {
        animateTo(
          navigateSwipeCommitOffset("left", width),
          "left",
          {
            direction: "left",
            duration_ms,
            distance_px,
            completed: true,
            cancelled: false,
            to_route: optsRef.current.to_routeLeft,
          },
          () => optsRef.current.onLeft?.(),
        );
        return;
      }
      if (direction === "right" && optsRef.current.onRight) {
        animateTo(
          navigateSwipeCommitOffset("right", width),
          "right",
          {
            direction: "right",
            duration_ms,
            distance_px,
            completed: true,
            cancelled: false,
            to_route: optsRef.current.to_routeRight,
          },
          () => optsRef.current.onRight?.(),
        );
        return;
      }

      if (swipeRef.current.engaged) {
        const snapDirection: "left" | "right" =
          navigateSwipeTranslateX(swipeRef.current) < 0 ? "left" : "right";
        animateTo(0, null, {
          direction: snapDirection,
          duration_ms,
          distance_px,
          completed: false,
          cancelled: true,
        });
        return;
      }
      reset();
    };

    root.addEventListener("touchstart", onTouchStart, { capture, passive: true });
    root.addEventListener("touchmove", onTouchMove, { capture, passive: false });
    root.addEventListener("touchend", onTouchEnd, { capture, passive: true });
    root.addEventListener("touchcancel", reset, { capture, passive: true });

    return () => {
      abortSettle();
      releaseGestures();
      root.removeEventListener("touchstart", onTouchStart, capture);
      root.removeEventListener("touchmove", onTouchMove, capture);
      root.removeEventListener("touchend", onTouchEnd, capture);
      root.removeEventListener("touchcancel", reset, capture);
    };
  }, [options.capture, ref]);

  const commit = useCallback((direction: SwipePageCommitDirection) => {
    commitRef.current(direction);
  }, []);

  const swiping = dragging || settling;
  const style: CSSProperties = {
    transform: dragX ? `translate3d(${dragX}px, 0, 0)` : undefined,
    transition: dragging
      ? "none"
      : settling
        ? `transform ${SWIPE_PAGE_COMMIT_MS}ms var(--ease-spring)`
        : undefined,
  };

  return { dragX, swiping, style, commit };
}
