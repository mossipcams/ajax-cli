import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import type { BrowserCockpitView, WebAction } from "@/shared/lib/types";
import { CONFIRM_TIMEOUT_MS, DROP_UNDO_MS } from "@/shared/lib/polling";
import { postOperation, requestId } from "@/shared/lib/api";
import {
  beginInteraction,
  cancelInteraction,
  endTapToFeedback,
  endTapToOperationComplete,
} from "@/shared/lib/posthog";

interface Props {
  actions: WebAction[];
  handle: string;
  /** Refreshed cockpit projection returned by a mutation. */
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  /** Surface the operation result for the result banner. */
  onResult?: (
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: { onUndo?: () => void; onCommit?: () => void },
  ) => void;
  /** Notify the parent a mutation finished (e.g. to refresh detail). */
  onMutated?: () => void;
  /** The task no longer exists (e.g. after Drop) — leave the detail page. */
  onDismiss?: () => void;
}

const REMEDIATION = new Set(["fix-ci", "resolve-merge-conflicts"]);

const actionRowStyle: CSSProperties = {
  display: "flex",
  flexWrap: "wrap",
  gap: "8px",
};

function actionClassName(
  action: WebAction,
  index: number,
  pendingAction: WebAction | null,
  runningAction: string | null,
): string {
  const classes = ["action"];
  // Destructive must never wear the accent primary fill (blue + red label).
  if (index === 0 && !action.destructive) classes.push("primary");
  if (pendingAction?.action === action.action) classes.push("confirming");
  if (runningAction === action.action) classes.push("is-running");
  if (REMEDIATION.has(action.action)) classes.push("remediation-action");
  return classes.join(" ");
}

export default function ActionBar({
  actions,
  handle,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
}: Props) {
  const [pendingAction, setPendingAction] = useState<WebAction | null>(null);
  const [runningAction, setRunningAction] = useState<string | null>(null);
  const confirmTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dropTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dropResolvedRef = useRef(false);
  const mountedRef = useRef(true);
  const interactionRef = useRef<string | null>(null);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (interactionRef.current) {
        cancelInteraction(interactionRef.current);
        interactionRef.current = null;
      }
      if (confirmTimerRef.current) clearTimeout(confirmTimerRef.current);
      if (dropTimerRef.current && !dropResolvedRef.current) return;
      if (dropTimerRef.current) clearTimeout(dropTimerRef.current);
    };
  }, []);

  const clearConfirm = useCallback(() => {
    if (confirmTimerRef.current) clearTimeout(confirmTimerRef.current);
    confirmTimerRef.current = null;
    setPendingAction(null);
  }, []);

  const clearConfirmAndInteraction = useCallback(() => {
    clearConfirm();
    if (interactionRef.current) {
      cancelInteraction(interactionRef.current);
      interactionRef.current = null;
    }
  }, [clearConfirm]);

  const clearDropTimer = useCallback(() => {
    if (dropTimerRef.current) clearTimeout(dropTimerRef.current);
    dropTimerRef.current = null;
  }, []);

  const label = (action: WebAction): string => {
    if (pendingAction?.action === action.action) return "Tap to confirm";
    if (runningAction === action.action) return `${action.label} …`;
    return action.label;
  };

  const run = async (action: WebAction, confirmed: boolean) => {
    if (mountedRef.current) setRunningAction(action.action);
    const interactionId = interactionRef.current;
    if (interactionId) endTapToFeedback(interactionId, "busy");
    try {
      const result = await postOperation({
        task_handle: handle,
        action: action.action,
        request_id: requestId(),
        confirmed,
        ...(action.branch_adoption ? { branch_adoption: action.branch_adoption } : {}),
      });
      if (result.response.cockpit) onCockpit?.(result.response.cockpit);
      if (result.ok) {
        if (interactionId) {
          endTapToOperationComplete(interactionId, { ok: true, op: action.action });
          interactionRef.current = null;
        }
        // Drop removes the task; refreshing this detail would 404. Leave instead.
        // If we unmounted during the undo window (operator switched tasks), commit
        // the Drop but do not navigate — the new task view is already active.
        if (action.action === "drop") {
          if (mountedRef.current) onDismiss?.();
        } else onMutated?.();
      } else {
        if (interactionId) {
          endTapToOperationComplete(interactionId, {
            ok: false,
            op: action.action,
            error_kind: "operation_failed",
          });
          interactionRef.current = null;
        }
        onResult?.(
          result.error?.message ?? "Action failed",
          result.response.output,
          true,
        );
      }
    } catch {
      if (interactionId) {
        endTapToOperationComplete(interactionId, {
          ok: false,
          op: action.action,
          error_kind: "network",
        });
        interactionRef.current = null;
      }
      onResult?.("Action failed — network error", null, true);
    } finally {
      if (mountedRef.current) setRunningAction(null);
    }
  };

  // Arm the delayed-Drop undo window. The toast's Undo cancels (no API); the
  // timer or the toast's auto-dismiss commits by running the real Drop.
  const armDrop = (action: WebAction) => {
    dropResolvedRef.current = false;
    setRunningAction("drop");
    const interactionId = interactionRef.current;
    if (interactionId) endTapToFeedback(interactionId, "banner");
    const commit = () => {
      if (dropResolvedRef.current) return;
      dropResolvedRef.current = true;
      clearDropTimer();
      void run(action, true);
    };
    const undo = () => {
      if (dropResolvedRef.current) return;
      dropResolvedRef.current = true;
      clearDropTimer();
      if (interactionId) {
        cancelInteraction(interactionId);
        interactionRef.current = null;
      }
      if (mountedRef.current) setRunningAction(null);
    };
    dropTimerRef.current = setTimeout(commit, DROP_UNDO_MS);
    onResult?.(`Dropping ${handle}…`, null, false, { onUndo: undo, onCommit: commit });
  };

  const handleClick = (action: WebAction) => {
    if (runningAction) return;
    const needsConfirm = action.destructive || action.confirmation_required;
    if (needsConfirm && pendingAction?.action !== action.action) {
      clearConfirm();
      const interactionId = beginInteraction(action.action);
      interactionRef.current = interactionId;
      setPendingAction(action);
      endTapToFeedback(interactionId, "confirm");
      confirmTimerRef.current = setTimeout(clearConfirmAndInteraction, CONFIRM_TIMEOUT_MS);
      return;
    }
    const retained = pendingAction?.action === action.action ? pendingAction : action;
    clearConfirm();
    const interactionId = beginInteraction(retained.action);
    interactionRef.current = interactionId;
    // Only Drop is delayed for pre-commit undo; other actions run immediately.
    if (retained.action === "drop") {
      armDrop(retained);
      return;
    }
    void run(retained, needsConfirm);
  };

  return (
    <div className="action-row" style={actionRowStyle}>
      {actions.map((action, index) => (
        <button
          key={action.action}
          type="button"
          className={actionClassName(action, index, pendingAction, runningAction)}
          data-action={action.action}
          data-task={handle}
          {...(action.destructive ? { "data-destructive": "true" } : {})}
          disabled={runningAction !== null && runningAction !== action.action}
          onClick={() => handleClick(action)}
        >
          {label(action)}
        </button>
      ))}
    </div>
  );
}
