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
  dropResolvedRef: { current: boolean };
};

type DropComposerCleanup = (handle: string) => void;

let dropComposerCleanup: DropComposerCleanup | null = null;

/** Registered by task-workspace while mounted; runs on committed Drop before dismiss. */
export function registerDropComposerCleanup(fn: DropComposerCleanup | null): void {
  dropComposerCleanup = fn;
}

function runDropComposerCleanup(handle: string): void {
  dropComposerCleanup?.(handle);
}

export function clearDropTimer(handles: DropUndoHandles) {
  if (handles.dropTimerRef.current) clearTimeout(handles.dropTimerRef.current);
  handles.dropTimerRef.current = null;
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
      ...(action.branch_adoption ? { branch_adoption: action.branch_adoption } : {}),
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

/** Arm the delayed-Drop undo window after shell confirm. */
export function armDropUndo(
  action: WebAction,
  handle: string,
  interactionId: string | null,
  callbacks: TaskMutationCallbacks,
  handles: DropUndoHandles,
  executeOperation: ExecuteTaskOperation = postOperation,
): void {
  handles.dropResolvedRef.current = false;
  if (interactionId) endTapToFeedback(interactionId, "banner");
  const commit = () => {
    if (handles.dropResolvedRef.current) return;
    handles.dropResolvedRef.current = true;
    clearDropTimer(handles);
    void runTaskAction(action, handle, true, interactionId, callbacks, executeOperation);
  };
  const undo = () => {
    if (handles.dropResolvedRef.current) return;
    handles.dropResolvedRef.current = true;
    clearDropTimer(handles);
    if (interactionId) {
      endTapToOperationComplete(interactionId, {
        ok: false,
        op: action.action,
        error_kind: "undo",
      });
    }
  };
  handles.dropTimerRef.current = setTimeout(commit, DROP_UNDO_MS);
  callbacks.onResult?.(`Dropping ${handle}…`, null, false, { onUndo: undo, onCommit: commit });
}

/** Proceed after shell confirm: Drop arms undo; other actions run immediately. */
export function commitConfirmedAction(
  action: WebAction,
  handle: string,
  interactionId: string,
  callbacks: TaskMutationCallbacks,
  dropHandles: DropUndoHandles,
  executeOperation: ExecuteTaskOperation = postOperation,
): void {
  if (action.action === "drop") {
    armDropUndo(action, handle, interactionId, callbacks, dropHandles, executeOperation);
    return;
  }
  void runTaskAction(action, handle, true, interactionId, callbacks, executeOperation);
}
