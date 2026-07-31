import { lazy, Suspense, useRef, type TouchEvent } from "react";
import type { BrowserCockpitView, BrowserTaskDetail } from "@/shared/lib/types";
import { statusMeta } from "@/shared/lib/state";
import {
  isTerminalGestureTarget,
  navigateSwipeEnd,
  navigateSwipeMove,
  navigateSwipeStart,
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
  const touchRef = useRef({ x: 0, y: 0, tracking: false, swipe: navigateSwipeStart() });

  const activityLine = (() => {
    const line = detail.agent_activity ?? detail.live_status_summary;
    return line && line !== detail.status_explanation ? line : null;
  })();

  function onTouchStart(event: TouchEvent) {
    if (isTerminalGestureTarget(event.target)) {
      touchRef.current.tracking = false;
      return;
    }
    const touch = event.changedTouches[0];
    if (!touch) return;
    touchRef.current = {
      x: touch.clientX,
      y: touch.clientY,
      tracking: true,
      swipe: navigateSwipeStart(),
    };
  }

  function onTouchMove(event: TouchEvent) {
    if (!touchRef.current.tracking) return;
    const touch = event.changedTouches[0];
    if (!touch) return;
    const dx = touch.clientX - touchRef.current.x;
    const dy = touch.clientY - touchRef.current.y;
    touchRef.current.swipe = navigateSwipeMove(touchRef.current.swipe, dx, dy);
  }

  function onTouchEnd() {
    if (!touchRef.current.tracking) return;
    const direction = navigateSwipeEnd(touchRef.current.swipe);
    touchRef.current.tracking = false;
    if (direction === "left") onOpenDiff?.();
  }

  return (
    <div
      className="task-detail"
      data-testid="task-detail"
      onTouchStart={onTouchStart}
      onTouchMove={onTouchMove}
      onTouchEnd={onTouchEnd}
      onTouchCancel={() => {
        touchRef.current.tracking = false;
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
