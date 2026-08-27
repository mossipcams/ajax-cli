import type { BrowserCockpitView, BrowserTaskDetail, WebAction } from "@/shared/lib/types";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { Button } from "@/shared/ui/button";
import {
  ActionBar,
  HarnessSwap,
  TaskMetaDetails,
  visibleTaskActions,
} from "@/features/task/public";
import { taskOffersOrchestrationChat } from "./taskWorkspaceRouting";

export interface TaskDetailsSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  panelId: string;
  mode: "chat" | "terminal";
  detail: BrowserTaskDetail;
  orchestrationChat?: boolean;
  harnessSwapDisabled?: boolean;
  onOpenDiff?: () => void;
  onOpenTerminal?: () => void;
  onOpenChat?: () => void;
  onSwappedAgent?: () => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: {
      onUndo?: () => void;
      onCommit?: () => void;
      pendingConfirm?: { action: WebAction; handle: string; interactionId: string };
    },
  ) => void;
  onMutated?: () => void;
  onDismiss?: () => void;
  pendingConfirmAction?: string | null;
  onCancelPendingConfirm?: () => void;
}

export default function TaskDetailsSheet({
  open,
  onOpenChange,
  panelId,
  mode,
  detail,
  orchestrationChat = false,
  harnessSwapDisabled = false,
  onOpenDiff,
  onOpenTerminal,
  onOpenChat,
  onSwappedAgent,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
  pendingConfirmAction = null,
  onCancelPendingConfirm,
}: TaskDetailsSheetProps) {
  if (!open) return null;

  const handle = detail.qualified_handle;
  const actions = visibleTaskActions(detail.actions);
  const showAjaxChat =
    mode === "terminal" &&
    orchestrationChat &&
    taskOffersOrchestrationChat(detail) &&
    Boolean(onOpenChat);
  const showAjaxTerminal = mode === "chat" && Boolean(onOpenTerminal);

  function close() {
    onOpenChange(false);
  }

  function handleHarnessSwapped() {
    onSwappedAgent?.();
    onMutated?.();
  }

  return (
    <FullscreenLayer zIndex={50}>
      <Sheet open onOpenChange={(next) => !next && close()}>
        <SheetContent asChild aria-describedby={undefined}>
          <div
            className="session-sheet-scrim"
            onPointerDown={(event) => {
              if (event.target === event.currentTarget) close();
            }}
          >
            <div
              className="session-details-sheet"
              id={panelId}
              data-testid="task-details-sheet"
              role="dialog"
              aria-modal="true"
              aria-label="Task details"
            >
              <div className="session-sheet-header">
                <SheetTitle asChild>
                  <h2>Task details</h2>
                </SheetTitle>
                <Button type="button" variant="secondary" className="session-sheet-close" onClick={close}>
                  Close
                </Button>
              </div>

              {showAjaxTerminal ? (
                <div
                  className="session-sheet-tools session-sheet-tools-primary"
                  data-testid="session-primary-tools"
                >
                  <Button
                    type="button"
                    variant="secondary"
                    data-testid="session-ajax-terminal"
                    onClick={() => {
                      close();
                      onOpenTerminal?.();
                    }}
                  >
                    Ajax terminal
                  </Button>
                  {onOpenDiff ? (
                    <Button type="button" variant="secondary" onClick={onOpenDiff}>
                      Show diff
                    </Button>
                  ) : null}
                </div>
              ) : null}

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
                      close();
                      onOpenChat?.();
                    }}
                  >
                    Ajax chat
                  </Button>
                </div>
              ) : null}

              <div className="session-details-body" data-testid="session-details-body">
                {mode === "chat" ? (
                  <header className="session-task-identity" data-testid="session-task-identity">
                    <h3 className="session-task-title">
                      {detail.title || detail.qualified_handle}
                    </h3>
                    <p className="session-task-handle">{detail.qualified_handle}</p>
                    <p className="session-task-branch">{detail.branch}</p>
                  </header>
                ) : null}

                {detail.runtime_observation_error ? (
                  <p className="session-sheet-warning" data-testid="session-observation-error">
                    Observation error: {detail.runtime_observation_error}
                  </p>
                ) : null}

                {mode === "chat" && detail.agent ? (
                  <HarnessSwap
                    handle={handle}
                    currentAgent={detail.agent}
                    disabled={harnessSwapDisabled}
                    onSwapped={handleHarnessSwapped}
                  />
                ) : null}

                <TaskMetaDetails
                  detail={detail}
                  embedded
                  hideBranch={mode === "chat"}
                  onResult={onResult}
                />

                {mode === "chat" && actions.length ? (
                  <div
                    className="session-sheet-actions session-sheet-actions-muted"
                    data-testid="session-quick-actions"
                  >
                    <ActionBar
                      actions={actions}
                      handle={handle}
                      onCockpit={onCockpit}
                      onResult={onResult}
                      onMutated={onMutated}
                      onDismiss={onDismiss}
                      pendingConfirmAction={pendingConfirmAction}
                      onCancelPendingConfirm={onCancelPendingConfirm}
                    />
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        </SheetContent>
      </Sheet>
    </FullscreenLayer>
  );
}
