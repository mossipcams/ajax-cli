import { type RefObject } from "react";
import type { BrowserCockpitView, BrowserTaskDetail, WebAction } from "@/shared/lib/types";
import type { RemoteResource } from "@/shared/lib/types";
import { TaskWorkspace } from "@/features/task-workspace/public";

export type TaskWorkspaceRouteKind = "task" | "session";

export interface TaskWorkspaceRouteProps {
  kind: TaskWorkspaceRouteKind;
  handle: string;
  orchestrationChat: boolean;
  detail: RemoteResource<BrowserTaskDetail>;
  reload: () => void;
  outletRef?: RefObject<HTMLElement | null>;
  outletClassName?: string;
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
  onDismiss?: () => void;
  pendingConfirmAction?: string | null;
  onCancelPendingConfirm?: () => void;
}

export default function TaskWorkspaceRoute({
  kind,
  handle,
  orchestrationChat,
  detail,
  reload,
  outletRef,
  outletClassName,
  onGo,
  onBack,
  onOpenDiff,
  onCockpit,
  onResult,
  onDismiss,
  pendingConfirmAction = null,
  onCancelPendingConfirm,
}: TaskWorkspaceRouteProps) {
  const mode = kind === "session" ? "chat" : "terminal";
  const outlet = kind === "session" ? "session" : "task";
  const testId = kind === "session" ? "outlet-session" : "outlet-task";

  return (
    <section
      ref={outletRef}
      className={outletClassName || undefined}
      data-outlet={outlet}
      data-testid={testId}
      data-handle={handle}
      aria-live="polite"
    >
      <TaskWorkspace
        handle={handle}
        mode={mode}
        detail={detail}
        orchestrationChat={orchestrationChat}
        onGo={onGo}
        onBack={onBack}
        onOpenDiff={onOpenDiff}
        onCockpit={onCockpit}
        onResult={onResult}
        onMutated={reload}
        onDismiss={onDismiss}
        onRetry={reload}
        pendingConfirmAction={pendingConfirmAction}
        onCancelPendingConfirm={onCancelPendingConfirm}
      />
    </section>
  );
}
