import { lazy, Suspense, useEffect, useRef, useState } from "react";
import type { BrowserCockpitView, BrowserTaskDetail } from "@/shared/lib/types";
import { statusMeta } from "@/shared/lib/state";
import {
  NAVIGATE_LONG_PRESS_MS,
  NAVIGATE_LONG_PRESS_MOVE_CANCEL_PX,
  navigateSwipeEnd,
  navigateSwipeMove,
  navigateSwipeStart,
  navigateSwipeTranslateX,
  type NavigateSwipeState,
} from "@/shared/gestures/navigateSwipe";
import { visibleTaskActions } from "./taskActions";
import ActionBar from "./ActionBar";
import TaskMetaDetails from "./TaskMetaDetails";

const TaskTerminal = lazy(() => import("./TaskTerminal"));

interface Props {
  detail: BrowserTaskDetail;
  onBack?: () => void;
  onOpenDiff?: () => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
  onDismiss?: () => void;
}

export default function TaskDetail({
  detail,
  onBack,
  onOpenDiff,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
}: Props) {
  const meta = statusMeta(detail.status);
  const actions = visibleTaskActions(detail.actions);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const originRef = useRef({ x: 0, y: 0 });
  const lastTouchRef = useRef({ x: 0, y: 0 });
  const pressRef = useRef({
    armingCancelled: false,
    armed: false,
    armTimer: 0 as ReturnType<typeof setTimeout> | 0,
  });
  const swipeRef = useRef<NavigateSwipeState>(navigateSwipeStart());
  // Inline App callbacks change every cockpit poll; keep listeners stable.
  const onOpenDiffRef = useRef(onOpenDiff);
  onOpenDiffRef.current = onOpenDiff;
  const [dragX, setDragX] = useState(0);
  const [dragging, setDragging] = useState(false);

  const activityLine = (() => {
    const line = detail.agent_activity ?? detail.live_status_summary;
    return line && line !== detail.status_explanation ? line : null;
  })();

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const clearArmTimer = () => {
      if (pressRef.current.armTimer) {
        clearTimeout(pressRef.current.armTimer);
        pressRef.current.armTimer = 0;
      }
    };

    const reset = () => {
      clearArmTimer();
      pressRef.current = { armingCancelled: false, armed: false, armTimer: 0 };
      swipeRef.current = navigateSwipeStart();
      setDragX(0);
      setDragging(false);
    };

    const onTouchStart = (event: TouchEvent) => {
      const touch = event.changedTouches[0] ?? event.touches[0];
      if (!touch) return;
      clearArmTimer();
      originRef.current = { x: touch.clientX, y: touch.clientY };
      lastTouchRef.current = { x: touch.clientX, y: touch.clientY };
      pressRef.current = { armingCancelled: false, armed: false, armTimer: 0 };
      swipeRef.current = navigateSwipeStart();
      setDragging(false);
      setDragX(0);
      pressRef.current.armTimer = setTimeout(() => {
        if (pressRef.current.armingCancelled) return;
        // Measure the swipe from the hold point, not the original touchstart.
        originRef.current = { ...lastTouchRef.current };
        swipeRef.current = navigateSwipeStart();
        pressRef.current.armed = true;
        pressRef.current.armTimer = 0;
      }, NAVIGATE_LONG_PRESS_MS);
    };

    const onTouchMove = (event: TouchEvent) => {
      const touch = event.changedTouches[0] ?? event.touches[0];
      if (!touch) return;
      lastTouchRef.current = { x: touch.clientX, y: touch.clientY };
      const dx = touch.clientX - originRef.current.x;
      const dy = touch.clientY - originRef.current.y;
      const press = pressRef.current;
      if (!press.armed) {
        if (press.armingCancelled) return;
        const movedPx = Math.max(Math.abs(dx), Math.abs(dy));
        if (movedPx > NAVIGATE_LONG_PRESS_MOVE_CANCEL_PX) {
          press.armingCancelled = true;
          clearArmTimer();
        }
        return;
      }
      const next = navigateSwipeMove(swipeRef.current, dx, dy);
      swipeRef.current = next;
      if (!next.engaged) return;
      // Own the gesture once horizontal intent is clear (including over the terminal).
      if (event.cancelable) event.preventDefault();
      setDragging(true);
      setDragX(navigateSwipeTranslateX(next));
    };

    const onTouchEnd = () => {
      const armed = pressRef.current.armed;
      const direction = navigateSwipeEnd(swipeRef.current);
      reset();
      if (armed && direction === "right") onOpenDiffRef.current?.();
    };

    root.addEventListener("touchstart", onTouchStart, { capture: true, passive: true });
    root.addEventListener("touchmove", onTouchMove, { capture: true, passive: false });
    root.addEventListener("touchend", onTouchEnd, { capture: true, passive: true });
    root.addEventListener("touchcancel", reset, { capture: true, passive: true });
    return () => {
      clearArmTimer();
      root.removeEventListener("touchstart", onTouchStart, true);
      root.removeEventListener("touchmove", onTouchMove, true);
      root.removeEventListener("touchend", onTouchEnd, true);
      root.removeEventListener("touchcancel", reset, true);
    };
  }, []);

  return (
    <div
      ref={rootRef}
      className={`task-detail${dragging ? " is-diff-swiping" : ""}`}
      data-testid="task-detail"
      style={{
        transform: dragX ? `translate3d(${dragX}px, 0, 0)` : undefined,
        transition: dragging ? "none" : "transform 180ms var(--ease, ease)",
      }}
    >
      <div
        className="detail-header"
        data-mobile-chrome="header"
        data-testid="mobile-chrome-header"
      >
        <button type="button" className="back" onClick={() => onBack?.()}>
          ← Back
        </button>
        <h1 className="detail-title">{detail.title || detail.qualified_handle}</h1>
        <span className={`interact-pill tone-${meta.tone}`}>{meta.label}</span>
      </div>

      <section
        className="interact-panel"
        data-mobile-chrome="actions"
        data-testid="mobile-chrome-actions"
      >
        {detail.runtime_observation_error ? (
          <p className="interact-warning" data-testid="observation-error">
            Observation error: {detail.runtime_observation_error}
          </p>
        ) : null}
        {detail.status_explanation ? (
          <p className="interact-summary">{detail.status_explanation}</p>
        ) : null}
        {activityLine ? (
          <p className="interact-summary interact-activity" data-testid="agent-activity">
            {activityLine}
          </p>
        ) : null}
        {actions.length ? (
          <ActionBar
            actions={actions}
            handle={detail.qualified_handle}
            onCockpit={onCockpit}
            onResult={onResult}
            onMutated={onMutated}
            onDismiss={onDismiss}
          />
        ) : null}
      </section>

      <div>
        <Suspense fallback={null}>
          <TaskTerminal handle={detail.qualified_handle} />
        </Suspense>
      </div>

      <TaskMetaDetails detail={detail} onResult={onResult} />
    </div>
  );
}
