import { lazy, Suspense, useRef } from "react";
import type { BrowserCockpitView, BrowserTaskCard, BrowserTaskDetail } from "@/shared/lib/types";
import { isAjaxWebSessionEnabled } from "@/shared/lib/ajaxWebSessionSetting";
import { statusMeta } from "@/shared/lib/state";
import { useSwipePageTransition } from "@/shared/hooks/useSwipePageTransition";
import AjaxWebSessionView from "@/features/session/AjaxWebSessionView";
import { visibleTaskActions } from "./taskActions";
import ActionBar from "./ActionBar";
import TaskMetaDetails from "./TaskMetaDetails";

// Keep TaskTerminal lazy so vite emits only app.js + terminal.js.
const TaskTerminal = lazy(() => import("./TaskTerminal"));

function isCursorAgent(agent: string): boolean {
  return agent.trim().toLowerCase() === "cursor";
}

interface Props {
  detail: BrowserTaskDetail;
  cockpitCards?: BrowserTaskCard[];
  onBack?: () => void;
  onOpenDiff?: () => void;
  onOpenTask?: (handle: string) => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
  onDismiss?: () => void;
}

export default function TaskDetail({
  detail,
  cockpitCards = [],
  onBack,
  onOpenDiff,
  onOpenTask,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
}: Props) {
  const meta = statusMeta(detail.status);
  const actions = visibleTaskActions(detail.actions);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const onOpenDiffRef = useRef(onOpenDiff);
  onOpenDiffRef.current = onOpenDiff;
  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;
  const { swiping, style, commit } = useSwipePageTransition(rootRef, {
    onLeft: () => onOpenDiffRef.current?.(),
    onRight: () => onBackRef.current?.(),
  });

  const activityLine = (() => {
    const line = detail.agent_activity ?? detail.live_status_summary;
    return line && line !== detail.status_explanation ? line : null;
  })();
  const showAjaxWebSession = isAjaxWebSessionEnabled() && isCursorAgent(detail.agent);

  return (
    <div
      ref={rootRef}
      className={`task-detail${swiping ? " is-diff-swiping" : ""}`}
      data-testid="task-detail"
      style={style}
    >
      <div
        className="detail-header"
        data-mobile-chrome="header"
        data-testid="mobile-chrome-header"
      >
        <button type="button" className="back" onClick={() => commit("right")}>
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
        {showAjaxWebSession ? (
          <AjaxWebSessionView
            handle={detail.qualified_handle}
            cockpitCards={cockpitCards}
            onOpenTask={onOpenTask}
          />
        ) : (
          <Suspense fallback={null}>
            <TaskTerminal handle={detail.qualified_handle} />
          </Suspense>
        )}
      </div>

      <TaskMetaDetails detail={detail} onResult={onResult} />
    </div>
  );
}
