import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type RefObject,
  type TransitionEvent as ReactTransitionEvent,
} from "react";
import {
  crossSlideEnteringOffset,
  crossSlideLeavingTarget,
  crossSlideRemainingPx,
  navigateSwipeCommitOffset,
  navigateSwipeEnd,
  navigateSwipeMove,
  navigateSwipeStart,
  navigateSwipeTranslateX,
  type NavigateSwipeState,
} from "@/shared/gestures/navigateSwipe";
import { gestureBusyGate } from "@/shared/lib/cockpitPoll";
import { parseRoute, type Route } from "@/shared/lib/routes";
import { type SwipeEnterDirection, setSwipeEnterDirection } from "@/shared/lib/swipeEnter";
import { captureSwipe, markNavigationStart } from "@/shared/lib/telemetry";
import { shouldSuppressPageSwipe } from "@/shared/lib/terminalSelecting";

export const SWIPE_PAGE_COMMIT_MS = 220;
/** Serial exit animation + destination enter keyframe budget cross-slide replaces. */
export const SERIAL_SWIPE_COMMIT_BUDGET_MS = SWIPE_PAGE_COMMIT_MS * 2;
/** ponytail: armed phase may wait for double-rAF before animating styles apply. */
export const CROSS_SLIDE_ARMED_SLACK_MS = 80;
const SWIPE_COMMIT_MIN_MS = 80;
const SWIPE_COMMIT_VELOCITY_FLOOR = 0.45;

export function computeSwipeCommitDurationMs(
  remainingPx: number,
  velocityPxPerMs: number,
  maxMs = SWIPE_PAGE_COMMIT_MS,
  minMs = SWIPE_COMMIT_MIN_MS,
): number {
  if (velocityPxPerMs > SWIPE_COMMIT_VELOCITY_FLOOR && remainingPx > 0) {
    return Math.round(Math.max(minMs, Math.min(maxMs, remainingPx / velocityPxPerMs)));
  }
  return maxMs;
}

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

type SwipeOutcome = {
  direction: "left" | "right";
  duration_ms: number;
  distance_px: number;
  completed: boolean;
  cancelled: boolean;
  to_route?: string;
};

type CrossSlideCommitParams = {
  direction: SwipeEnterDirection;
  dragX: number;
  pageWidth: number;
  fromRoute: string;
  swipeOutcome: SwipeOutcome;
  navigate: () => void;
};

type ActiveCrossSlide = {
  leavingRoute: Route;
  direction: SwipeEnterDirection;
  leavingX: number;
  enteringX: number;
  commitMs: number;
  phase: "armed" | "animating";
  swipeOutcome: SwipeOutcome;
  fromRoute: string;
  pageWidth: number;
  settleStartedAt: number;
};

export type PageCrossSlideContextValue = {
  active: boolean;
  isBusy: () => boolean;
  leavingRoute: Route | null;
  beginCommit: (params: CrossSlideCommitParams) => boolean;
  paneStyle: (role: "leaving" | "entering") => CSSProperties;
  onEnteringTransitionEnd: (event: ReactTransitionEvent) => void;
};

const PageCrossSlideContext = createContext<PageCrossSlideContextValue | null>(null);

function swipeVelocity(distance_px: number, duration_ms: number): number {
  if (duration_ms <= 0) return 0;
  return Math.round((distance_px / duration_ms) * 1000) / 1000;
}

function readTouch(event: TouchEvent): { x: number; y: number } | null {
  const touch = event.changedTouches[0] ?? event.touches[0];
  if (!touch) return null;
  return { x: touch.clientX, y: touch.clientY };
}

/** Armed styles must paint before animating styles apply (double rAF); jsdom uses setTimeout. */
export function scheduleCrossSlideAnimatingFlip(callback: () => void): void {
  const useTimeoutPath =
    typeof requestAnimationFrame !== "function" ||
    (typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent));

  if (useTimeoutPath) {
    window.setTimeout(callback, 0);
    return;
  }

  requestAnimationFrame(() => {
    requestAnimationFrame(callback);
  });
}

export function PageCrossSlideProvider({ children }: { children: ReactNode }) {
  const [active, setActive] = useState<ActiveCrossSlide | null>(null);
  const activeRef = useRef<ActiveCrossSlide | null>(null);
  activeRef.current = active;
  const settleHeldRef = useRef(false);
  const settleFallbackRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearSettleFallback = () => {
    if (settleFallbackRef.current !== null) {
      window.clearTimeout(settleFallbackRef.current);
      settleFallbackRef.current = null;
    }
  };

  const releaseSettleGesture = () => {
    if (settleHeldRef.current) {
      settleHeldRef.current = false;
      gestureBusyGate.end();
    }
  };

  const holdSettleGesture = () => {
    if (!settleHeldRef.current) {
      settleHeldRef.current = true;
      gestureBusyGate.begin();
    }
  };

  const finishCrossSlide = useCallback(() => {
    clearSettleFallback();
    const slide = activeRef.current;
    if (!slide) return;
    const settle_ms = Math.round(performance.now() - slide.settleStartedAt);
    captureSwipe({
      direction: slide.swipeOutcome.direction,
      duration_ms: slide.swipeOutcome.duration_ms,
      distance_px: slide.swipeOutcome.distance_px,
      page_width_px: slide.pageWidth,
      velocity_px_per_ms: swipeVelocity(
        slide.swipeOutcome.distance_px,
        slide.swipeOutcome.duration_ms,
      ),
      completed: slide.swipeOutcome.completed,
      cancelled: slide.swipeOutcome.cancelled,
      settle_ms,
      from_route: slide.fromRoute,
      ...(slide.swipeOutcome.to_route ? { to_route: slide.swipeOutcome.to_route } : {}),
    });
    activeRef.current = null;
    setActive(null);
    releaseSettleGesture();
  }, []);

  const beginCommit = useCallback((params: CrossSlideCommitParams) => {
    if (activeRef.current) return false;
    const leavingRoute = parseRoute(window.location.hash);
    const remaining = crossSlideRemainingPx(
      params.direction,
      params.dragX,
      params.pageWidth,
    );
    const commitMs = computeSwipeCommitDurationMs(
      remaining,
      swipeVelocity(params.swipeOutcome.distance_px, params.swipeOutcome.duration_ms),
    );
    holdSettleGesture();
    const nextActive: ActiveCrossSlide = {
      leavingRoute,
      direction: params.direction,
      leavingX: params.dragX,
      enteringX: crossSlideEnteringOffset(params.direction, params.dragX, params.pageWidth),
      commitMs,
      phase: "armed",
      swipeOutcome: params.swipeOutcome,
      fromRoute: params.fromRoute,
      pageWidth: params.pageWidth,
      settleStartedAt: performance.now(),
    };
    activeRef.current = nextActive;
    setActive(nextActive);
    markNavigationStart(params.fromRoute, "swipe");
    params.navigate();

    const scheduleSettleFallback = (delayMs: number) => {
      clearSettleFallback();
      settleFallbackRef.current = window.setTimeout(() => {
        settleFallbackRef.current = null;
        finishCrossSlide();
      }, delayMs);
    };

    // Schedule before armed→animating flip so a deferred/skipped rAF cannot leave active stuck.
    scheduleSettleFallback(commitMs + 40 + CROSS_SLIDE_ARMED_SLACK_MS);

    scheduleCrossSlideAnimatingFlip(() => {
      setActive((current) => {
        if (!current || current.phase !== "armed") return current;
        const animating = {
          ...current,
          phase: "animating" as const,
          leavingX: crossSlideLeavingTarget(current.direction, current.pageWidth),
          enteringX: 0,
        };
        activeRef.current = animating;
        return animating;
      });
      scheduleSettleFallback(commitMs + 40);
    });
    return true;
  }, [finishCrossSlide]);

  const paneStyle = useCallback(
    (role: "leaving" | "entering"): CSSProperties => {
      if (!active) return {};
      const x = role === "leaving" ? active.leavingX : active.enteringX;
      return {
        transform: `translate3d(${x}px, 0, 0)`,
        transition:
          active.phase === "animating"
            ? `transform ${active.commitMs}ms var(--ease-spring)`
            : "none",
      };
    },
    [active],
  );

  const onEnteringTransitionEnd = useCallback(
    (event: ReactTransitionEvent) => {
      if (!activeRef.current || event.propertyName !== "transform") return;
      finishCrossSlide();
    },
    [finishCrossSlide],
  );

  const isBusy = useCallback(() => activeRef.current !== null, []);

  const value: PageCrossSlideContextValue = {
    active: active !== null,
    isBusy,
    leavingRoute: active?.leavingRoute ?? null,
    beginCommit,
    paneStyle,
    onEnteringTransitionEnd,
  };

  return createElement(PageCrossSlideContext.Provider, { value }, children);
}

function usePageCrossSlide(): PageCrossSlideContextValue | null {
  return useContext(PageCrossSlideContext);
}

export { usePageCrossSlide };

export function useSwipePageTransition(
  ref: RefObject<HTMLElement | null>,
  options: SwipePageTransitionOptions,
): SwipePageTransitionResult {
  const crossSlide = usePageCrossSlide();
  const crossSlideRef = useRef(crossSlide);
  crossSlideRef.current = crossSlide;
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

  const settleDurationRef = useRef(SWIPE_PAGE_COMMIT_MS);

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
    if (crossSlide?.active) return;
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

    const tryCrossSlideCommit = (
      direction: SwipeEnterDirection,
      swipeOutcome: SwipeOutcome,
      navigate: () => void,
    ): boolean => {
      const controller = crossSlideRef.current;
      if (!controller || controller.isBusy()) return false;
      return controller.beginCommit({
        direction,
        dragX: swipeRef.current.engaged
          ? navigateSwipeTranslateX(swipeRef.current)
          : 0,
        pageWidth: pageWidth(),
        fromRoute: readFromRoute(),
        swipeOutcome,
        navigate,
      });
    };

    const animateTo = (
      targetX: number,
      direction: SwipeEnterDirection | null,
      swipeOutcome: SwipeOutcome | null,
      then?: () => void,
      commitDurationMs = SWIPE_PAGE_COMMIT_MS,
    ) => {
      if (settlingRef.current) return;
      if (direction && then && swipeOutcome) {
        if (tryCrossSlideCommit(direction, swipeOutcome, then)) {
          reset();
          return;
        }
        if (crossSlideRef.current?.isBusy()) return;
      }
      settlingRef.current = true;
      settleDurationRef.current = commitDurationMs;
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

      settleTimer = window.setTimeout(finish, commitDurationMs + 40);
      settleOnTransitionEnd = onTransitionEnd;
      root.addEventListener("transitionend", onTransitionEnd);
    };

    commitRef.current = (direction: SwipePageCommitDirection) => {
      const width = pageWidth();
      const swipeOutcome: SwipeOutcome = {
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
      // Capture runs before terminal bubble: refuse to arm while selecting, or while
      // a double-tap is pending on the terminal (second contact).
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
      const dragX = swipeRef.current.engaged
        ? navigateSwipeTranslateX(swipeRef.current)
        : 0;
      const cancelDurationMs = computeSwipeCommitDurationMs(
        Math.abs(dragX),
        swipeVelocity(distance_px, duration_ms),
      );

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
        animateTo(
          0,
          null,
          {
            direction: snapDirection,
            duration_ms,
            distance_px,
            completed: false,
            cancelled: true,
          },
          undefined,
          cancelDurationMs,
        );
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
  }, [crossSlide?.active, options.capture, ref]);

  const commit = useCallback((direction: SwipePageCommitDirection) => {
    commitRef.current(direction);
  }, []);

  if (crossSlide?.active) {
    return { dragX: 0, swiping: false, style: {}, commit };
  }

  const swiping = dragging || settling;
  const style: CSSProperties = {
    transform: dragX ? `translate3d(${dragX}px, 0, 0)` : undefined,
    transition: dragging
      ? "none"
      : settling
        ? `transform ${settleDurationRef.current}ms var(--ease-spring)`
        : undefined,
  };

  return { dragX, swiping, style, commit };
}
