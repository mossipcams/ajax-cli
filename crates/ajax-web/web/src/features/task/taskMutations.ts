import type { BrowserCockpitView, WebAction } from "@/shared/lib/types";
import { DROP_UNDO_MS } from "@/shared/lib/polling";
import { postOperation, requestId } from "@/shared/lib/api";
import type { ExecuteTaskOperation } from "./useTaskOperationMutation";
import { operatorErrorPresentation } from "@/shared/lib/errorRecovery";
import { endTapToFeedback, endTapToOperationComplete } from "@/shared/lib/telemetry";

export type TaskMutationCallbacks = {
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: { onUndo?: () => void; onCommit?: () => void },
  ) => void;
  onMutated?: () => void;
  onDismiss?: () => void;
  isMounted?: () => boolean;
};

export type DropUndoHandles = {
  dropTimerRef: { current: ReturnType<typeof setTimeout> | null };
};

type DropComposerCleanup = (handle: string) => void;

let dropComposerCleanup: DropComposerCleanup | null = null;

export function registerDropComposerCleanup(fn: DropComposerCleanup | null): void {
  dropComposerCleanup = fn;
}

function runDropComposerCleanup(handle: string): void {
  dropComposerCleanup?.(handle);
}

export async function runTaskAction(
  action: WebAction,
  handle: string,
  confirmed: boolean,
  interactionId: string | null,
  callbacks: TaskMutationCallbacks,
  executeOperation: ExecuteTaskOperation = postOperation,
): Promise<void> {
  if (interactionId) endTapToFeedback(interactionId, "busy");
  try {
    const result = await executeOperation({
      task_handle: handle,
      action: action.action,
      request_id: requestId(),
      confirmed,
    });
    if (result.response.cockpit) callbacks.onCockpit?.(result.response.cockpit);
    if (result.ok) {
      if (interactionId) {
        endTapToOperationComplete(interactionId, { ok: true, op: action.action });
      }
      if (action.action === "drop") {
        runDropComposerCleanup(handle);
        if (callbacks.isMounted?.() !== false) callbacks.onDismiss?.();
      } else {
        callbacks.onMutated?.();
      }
    } else {
      const presentation = operatorErrorPresentation(result.error ?? result.response);
      if (interactionId) {
        endTapToOperationComplete(interactionId, {
          ok: false,
          op: action.action,
          error_kind: presentation.telemetryKind,
        });
      }
      callbacks.onResult?.(presentation.message, result.response.output, true);
    }
  } catch {
    const presentation = operatorErrorPresentation({ kind: "network", message: "" });
    if (interactionId) {
      endTapToOperationComplete(interactionId, {
        ok: false,
        op: action.action,
        error_kind: presentation.telemetryKind,
      });
    }
    callbacks.onResult?.(presentation.message, null, true);
  }
}

export function armDropUndo(
  action: WebAction,
  handle: string,
  interactionId: string | null,
  callbacks: TaskMutationCallbacks,
  handles: DropUndoHandles,
  executeOperation: ExecuteTaskOperation = postOperation,
): void {
  let resolved = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  if (interactionId) endTapToFeedback(interactionId, "banner");
  const settle = () => {
    resolved = true;
    if (timer) clearTimeout(timer);
    if (handles.dropTimerRef.current === timer) handles.dropTimerRef.current = null;
    document.removeEventListener("visibilitychange", onVisibilityChange);
    window.removeEventListener("pagehide", commit);
  };
  const commit = () => {
    if (resolved) return;
    settle();
    void runTaskAction(action, handle, true, interactionId, callbacks, executeOperation);
  };
  const onVisibilityChange = () => {
    if (document.visibilityState === "hidden") commit();
  };
  const undo = () => {
    if (resolved) return;
    settle();
    if (interactionId) {
      endTapToOperationComplete(interactionId, {
        ok: false,
        op: action.action,
        error_kind: "undo",
      });
    }
  };
  document.addEventListener("visibilitychange", onVisibilityChange);
  window.addEventListener("pagehide", commit);
  timer = setTimeout(commit, DROP_UNDO_MS);
  handles.dropTimerRef.current = timer;
  const message =
    action.action === "drop" ? `Dropping ${handle}…` : `${action.label} for ${handle}…`;
  callbacks.onResult?.(message, null, false, { onUndo: undo, onCommit: commit });
}

const DEFERRED_ACTIONS = new Set(["drop", "create-pr", "merge-pr"]);

export function commitConfirmedAction(
  action: WebAction,
  handle: string,
  interactionId: string,
  callbacks: TaskMutationCallbacks,
  dropHandles: DropUndoHandles,
  executeOperation: ExecuteTaskOperation = postOperation,
): void {
  if (DEFERRED_ACTIONS.has(action.action)) {
    armDropUndo(action, handle, interactionId, callbacks, dropHandles, executeOperation);
    return;
  }
  void runTaskAction(action, handle, true, interactionId, callbacks, executeOperation);
}
