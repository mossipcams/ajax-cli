import { lazy, Suspense, useRef } from "react";
import type { BrowserCockpitView, BrowserTaskDetail } from "@/shared/lib/types";
import { useSwipePageTransition } from "@/shared/hooks/useSwipePageTransition";
import { taskHash } from "@/shared/lib/routes";
import { ActionBar, visibleTaskActions } from "@/features/task/public";
import TaskWorkspaceHeader from "./TaskWorkspaceHeader";

const TaskTerminal = lazy(() => import("@/features/terminal/TaskTerminal"));

interface Props {
  detail: BrowserTaskDetail;
  onBack?: () => void;
  onOpenDiff?: () => void;
  onOpenDetails?: () => void;
  detailsOpen?: boolean;
  detailsPanelId?: string;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
  onDismiss?: () => void;
  pendingConfirmAction?: string | null;
  onCancelPendingConfirm?: () => void;
}

export default function TaskTerminalView({
  detail,
  onBack,
  onOpenDiff,
  onOpenDetails,
  detailsOpen = false,
  detailsPanelId,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
  pendingConfirmAction = null,
  onCancelPendingConfirm,
}: Props) {
  const actions = visibleTaskActions(detail.actions);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const onOpenDiffRef = useRef(onOpenDiff);
  onOpenDiffRef.current = onOpenDiff;
  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;
  const { swiping, style, commit } = useSwipePageTransition(rootRef, {
    from_route: taskHash(detail.qualified_handle),
    onLeft: () => onOpenDiffRef.current?.(),
    onRight: () => onBackRef.current?.(),
  });

  const activityLine = (() => {
    const line = detail.agent_activity ?? detail.live_status_summary;
    return line && line !== detail.status_explanation ? line : null;
  })();

  return (
    <div
      ref={rootRef}
      className={`task-detail${swiping ? " is-diff-swiping" : ""}`}
      data-testid="task-detail"
      {...(detailsOpen ? { "data-task-details-open": "" } : {})}
      style={style}
    >
      <TaskWorkspaceHeader
        detail={detail}
        onBack={() => commit("right")}
        onOpenDetails={onOpenDetails}
        detailsOpen={detailsOpen}
        detailsPanelId={detailsPanelId}
      />

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
            pendingConfirmAction={pendingConfirmAction}
            onCancelPendingConfirm={onCancelPendingConfirm}
          />
        ) : null}
      </section>

      <div>
        <Suspense fallback={null}>
          <TaskTerminal handle={detail.qualified_handle} />
        </Suspense>
      </div>

      {onOpenDetails ? (
        <div className="task-meta-chrome">
          <div className="meta-details">
            <button
              type="button"
              className="meta-details-trigger"
              data-testid="task-meta-details-trigger"
              aria-expanded={detailsOpen}
              {...(detailsPanelId ? { "aria-controls": detailsPanelId } : {})}
              onClick={onOpenDetails}
            >
              Task details
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
