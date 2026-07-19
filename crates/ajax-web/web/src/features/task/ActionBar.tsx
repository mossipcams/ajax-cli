import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import type { BrowserCockpitView, WebAction } from "@/shared/lib/types";
import { CONFIRM_TIMEOUT_MS, DROP_UNDO_MS } from "@/shared/lib/polling";
import { postOperation, requestId } from "@/shared/lib/api";

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
  pendingAction: string | null,
  runningAction: string | null,
): string {
  const classes = ["action"];
  if (index === 0) classes.push("primary");
  if (pendingAction === action.action) classes.push("confirming");
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
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [runningAction, setRunningAction] = useState<string | null>(null);
  const confirmTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dropTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dropResolvedRef = useRef(false);

  useEffect(() => {
    return () => {
      if (confirmTimerRef.current) clearTimeout(confirmTimerRef.current);
      if (dropTimerRef.current) clearTimeout(dropTimerRef.current);
    };
  }, []);

  const clearConfirm = useCallback(() => {
    if (confirmTimerRef.current) clearTimeout(confirmTimerRef.current);
    confirmTimerRef.current = null;
    setPendingAction(null);
  }, []);

  const clearDropTimer = useCallback(() => {
    if (dropTimerRef.current) clearTimeout(dropTimerRef.current);
    dropTimerRef.current = null;
  }, []);

  const label = (action: WebAction): string => {
    if (pendingAction === action.action) return "Tap to confirm";
    if (runningAction === action.action) return `${action.label} …`;
    return action.label;
  };

  const run = async (action: WebAction) => {
    setRunningAction(action.action);
    try {
      const result = await postOperation({
        task_handle: handle,
        action: action.action,
        request_id: requestId(),
      });
      if (result.response.cockpit) onCockpit?.(result.response.cockpit);
      if (result.ok) {
        onResult?.(`${action.label} completed`, result.response.output, false);
        // Drop removes the task; refreshing this detail would 404. Leave instead.
        if (action.action === "drop") onDismiss?.();
        else onMutated?.();
      } else {
        onResult?.(
          result.error?.message ?? "Action failed",
          result.response.output,
          true,
        );
      }
    } catch {
      onResult?.("Action failed — network error", null, true);
    } finally {
      setRunningAction(null);
    }
  };

  // Arm the delayed-Drop undo window. The toast's Undo cancels (no API); the
  // timer or the toast's auto-dismiss commits by running the real Drop.
  const armDrop = (action: WebAction) => {
    dropResolvedRef.current = false;
    setRunningAction("drop");
    const commit = () => {
      if (dropResolvedRef.current) return;
      dropResolvedRef.current = true;
      clearDropTimer();
      void run(action);
    };
    const undo = () => {
      if (dropResolvedRef.current) return;
      dropResolvedRef.current = true;
      clearDropTimer();
      setRunningAction(null);
    };
    dropTimerRef.current = setTimeout(commit, DROP_UNDO_MS);
    onResult?.(`Dropping ${handle}…`, null, false, { onUndo: undo, onCommit: commit });
  };

  const handleClick = (action: WebAction) => {
    if (runningAction) return;
    const needsConfirm = action.destructive || action.confirmation_required;
    if (needsConfirm && pendingAction !== action.action) {
      clearConfirm();
      setPendingAction(action.action);
      confirmTimerRef.current = setTimeout(clearConfirm, CONFIRM_TIMEOUT_MS);
      return;
    }
    clearConfirm();
    // Only Drop is delayed for pre-commit undo; other actions run immediately.
    if (action.action === "drop") {
      armDrop(action);
      return;
    }
    void run(action);
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
