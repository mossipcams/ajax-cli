import { lazy, Suspense, useRef, useState } from "react";
import type { BrowserCockpitView, BrowserTaskDetail } from "@/shared/lib/types";
import { statusMeta } from "@/shared/lib/state";
import { useSwipePageTransition } from "@/shared/hooks/useSwipePageTransition";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { Button } from "@/shared/ui/button";
import { visibleTaskActions } from "./taskActions";
import ActionBar from "./ActionBar";
import TaskMetaDetails from "./TaskMetaDetails";

const TaskTerminal = lazy(() => import("./TaskTerminal"));

interface Props {
  detail: BrowserTaskDetail;
  orchestrationChat?: boolean;
  onBack?: () => void;
  onOpenDiff?: () => void;
  onOpenChat?: () => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
  onDismiss?: () => void;
  pendingConfirmAction?: string | null;
  onCancelPendingConfirm?: () => void;
}

export default function TaskDetail({
  detail,
  orchestrationChat = false,
  onBack,
  onOpenDiff,
  onOpenChat,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
  pendingConfirmAction = null,
  onCancelPendingConfirm,
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

  const showAjaxChat =
    orchestrationChat && detail.session_capable !== false && Boolean(onOpenChat);
  const [detailsOpen, setDetailsOpen] = useState(false);

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
        <div className="detail-header-controls">
          <button
            type="button"
            className="session-head-details"
            data-testid="task-details"
            onClick={() => setDetailsOpen(true)}
          >
            Details
          </button>
          <span className={`interact-pill tone-${meta.tone}`}>{meta.label}</span>
        </div>
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

      <div className="task-meta-chrome">
        <TaskMetaDetails
          detail={detail}
          onResult={onResult}
          showAjaxChat={showAjaxChat}
          onOpenChat={onOpenChat}
        />
      </div>

      {detailsOpen ? (
        <FullscreenLayer zIndex={50}>
          <Sheet open onOpenChange={(open) => !open && setDetailsOpen(false)}>
            <SheetContent asChild aria-describedby={undefined}>
              <div
                className="session-sheet-scrim"
                onPointerDown={(event) => {
                  if (event.target === event.currentTarget) setDetailsOpen(false);
                }}
              >
                <div
                  className="session-details-sheet"
                  data-testid="task-details-sheet"
                  role="dialog"
                  aria-modal="true"
                  aria-label="Task details"
                >
                  <div className="session-sheet-header">
                    <SheetTitle asChild>
                      <h2>Task details</h2>
                    </SheetTitle>
                    <Button type="button" variant="secondary" onClick={() => setDetailsOpen(false)}>
                      Close
                    </Button>
                  </div>

                  <div className="session-details-body">
                    {showAjaxChat ? (
                      <div
                        className="session-sheet-tools session-sheet-tools-primary"
                        data-testid="task-primary-tools"
                      >
                        <Button
                          type="button"
                          variant="secondary"
                          data-testid="task-ajax-chat"
                          onClick={() => {
                            setDetailsOpen(false);
                            onOpenChat?.();
                          }}
                        >
                          Ajax chat
                        </Button>
                      </div>
                    ) : null}
                    <TaskMetaDetails detail={detail} onResult={onResult} embedded />
                  </div>
                </div>
              </div>
            </SheetContent>
          </Sheet>
        </FullscreenLayer>
      ) : null}
    </div>
  );
}
