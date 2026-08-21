import { useEffect, useId, useState, type ReactNode } from "react";
import type { BrowserCockpitView, BrowserTaskDetail, RemoteResource, WebAction } from "@/shared/lib/types";
import { ChatSurface } from "@/features/chat/public";
import { ActionBar, TaskLoadError, visibleTaskActions } from "@/features/task/public";
import TaskTerminalView from "./TaskTerminalView";
import Skeleton from "@/shared/ui/Skeleton";
import { sessionHash, taskHash } from "@/shared/lib/routes";
import { clearSessionOutbox } from "@/shared/lib/webSessionTransport";
import TaskDetailsSheet from "./TaskDetailsSheet";
import TaskWorkspaceHeader from "./TaskWorkspaceHeader";
import {
  clearTaskTerminalPreferred,
  writeTaskTerminalPreferred,
} from "./taskViewPreference";
import { shouldRedirectSessionToTerminal } from "./taskWorkspaceRouting";

export type TaskWorkspaceMode = "chat" | "terminal";

export interface TaskWorkspaceProps {
  handle: string;
  mode: TaskWorkspaceMode;
  detail: RemoteResource<BrowserTaskDetail>;
  orchestrationChat: boolean;
  onGo: (hash: string) => void;
  onBack: () => void;
  onOpenDiff: () => void;
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
  onRetry?: () => void;
  pendingConfirmAction?: string | null;
  onCancelPendingConfirm?: () => void;
}

export default function TaskWorkspace({
  handle,
  mode,
  detail,
  orchestrationChat,
  onGo,
  onBack,
  onOpenDiff,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
  onRetry,
  pendingConfirmAction = null,
  onCancelPendingConfirm,
}: TaskWorkspaceProps) {
  const detailsPanelId = useId();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [sessionBusy, setSessionBusy] = useState(false);

  useEffect(() => {
    if (mode !== "chat") return;
    if (detail.status !== "ready" || !detail.data) return;
    if (!shouldRedirectSessionToTerminal(handle, detail.data)) return;
    onGo(taskHash(handle));
  }, [mode, handle, detail.status, detail.data, onGo]);

  // Drop confirm uses the shell ResultPanel (z-index 40); close the details
  // sheet (z-index 50) so Confirm is reachable without raising ResultPanel.
  useEffect(() => {
    if (pendingConfirmAction === "drop") setDetailsOpen(false);
  }, [pendingConfirmAction]);

  const taskDetail = detail.data;
  const actions = taskDetail ? visibleTaskActions(taskDetail.actions) : [];
  const safeActions = actions.filter((action) => !action.destructive);

  let headActions: ReactNode = null;
  if (mode === "chat" && taskDetail && safeActions.length) {
    headActions = (
      <div data-testid="session-head-actions">
        <ActionBar
          actions={safeActions}
          handle={taskDetail.qualified_handle ?? handle}
          onCockpit={onCockpit}
          onResult={onResult}
          onMutated={onMutated}
          onDismiss={onDismiss}
          pendingConfirmAction={pendingConfirmAction}
          onCancelPendingConfirm={onCancelPendingConfirm}
        />
      </div>
    );
  }

  const detailsSheet =
    taskDetail ? (
      <TaskDetailsSheet
        open={detailsOpen}
        onOpenChange={setDetailsOpen}
        panelId={detailsPanelId}
        mode={mode}
        detail={taskDetail}
        orchestrationChat={orchestrationChat}
        harnessSwapDisabled={sessionBusy}
        onOpenDiff={onOpenDiff}
        onOpenTerminal={
          mode === "chat"
            ? () => {
                writeTaskTerminalPreferred(handle);
                onGo(taskHash(handle));
              }
            : undefined
        }
        onOpenChat={
          mode === "terminal"
            ? () => {
                clearTaskTerminalPreferred(handle);
                onGo(sessionHash(handle));
              }
            : undefined
        }
        onSwappedAgent={mode === "chat" ? () => clearSessionOutbox(handle) : undefined}
        onCockpit={onCockpit}
        onResult={onResult}
        onMutated={onMutated}
        onDismiss={onDismiss}
        pendingConfirmAction={pendingConfirmAction}
        onCancelPendingConfirm={onCancelPendingConfirm}
      />
    ) : null;

  if (mode === "terminal" && detail.status === "loading") {
    return <Skeleton testid="task-skeleton" rows={6} />;
  }

  if (mode === "chat") {
    if (detail.status === "error" || (detail.status !== "loading" && !taskDetail)) {
      return (
        <TaskLoadError
          message={detail.error?.message ?? "Task not found"}
          onRetry={() => onRetry?.()}
        />
      );
    }

    return (
      <>
        <ChatSurface
          handle={handle}
          detail={taskDetail}
          detailStatus={detail.status}
          onBack={onBack}
          onOpenDiff={onOpenDiff}
          onMutated={onMutated}
          headActions={headActions}
          workspaceHeader={
            <TaskWorkspaceHeader
              detail={taskDetail}
              handle={handle}
              showStatusPill={false}
              onBack={onBack}
              onOpenDetails={taskDetail ? () => setDetailsOpen(true) : undefined}
              detailsOpen={detailsOpen}
              detailsPanelId={detailsPanelId}
              detailsTestId="session-details"
            />
          }
          onSessionActivity={({ busy }) => {
            setSessionBusy(busy);
          }}
        />
        {detailsSheet}
      </>
    );
  }

  if (taskDetail) {
    return (
      <>
        <TaskTerminalView
          detail={taskDetail}
          onBack={onBack}
          onOpenDiff={onOpenDiff}
          onCockpit={onCockpit}
          onResult={onResult}
          onMutated={onMutated}
          onDismiss={onDismiss}
          onOpenDetails={() => setDetailsOpen(true)}
          detailsOpen={detailsOpen}
          detailsPanelId={detailsPanelId}
          pendingConfirmAction={pendingConfirmAction}
          onCancelPendingConfirm={onCancelPendingConfirm}
        />
        {detailsSheet}
      </>
    );
  }

  return (
    <TaskLoadError
      message={detail.error?.message ?? "Request failed"}
      onRetry={() => onRetry?.()}
    />
  );
}
