import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import type { BrowserCockpitView, WebAction } from "@/shared/lib/types";
import {
  beginInteraction,
  endTapToFeedback,
  endTapToOperationComplete,
} from "@/shared/lib/telemetry";
import { runTaskAction, type TaskMutationCallbacks } from "./taskMutations";

export type PendingConfirmRequest = {
  action: WebAction;
  handle: string;
  interactionId: string;
};

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
    options?: {
      onUndo?: () => void;
      onCommit?: () => void;
      pendingConfirm?: PendingConfirmRequest;
    },
  ) => void;
  /** Notify the parent a mutation finished (e.g. to refresh detail). */
  onMutated?: () => void;
  /** The task no longer exists (e.g. after Drop) — leave the detail page. */
  onDismiss?: () => void;
  /** Shell confirm currently armed (`drop`, etc.). Sibling taps must not POST. */
  pendingConfirmAction?: string | null;
  /** Cancel that confirm when a different action is chosen. */
  onCancelPendingConfirm?: () => void;
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
  runningAction: string | null,
): string {
  const classes = ["action"];
  // Destructive must never wear the accent primary fill (blue + red label).
  if (index === 0 && !action.destructive) classes.push("primary");
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
  pendingConfirmAction = null,
  onCancelPendingConfirm,
}: Props) {
  const [runningAction, setRunningAction] = useState<string | null>(null);
  const mountedRef = useRef(true);
  const interactionRef = useRef<string | null>(null);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (interactionRef.current) {
        endTapToOperationComplete(interactionRef.current, {
          ok: false,
          error_kind: "unmount",
        });
        interactionRef.current = null;
      }
    };
  }, []);

  const mutationCallbacks = useCallback(
    (): TaskMutationCallbacks => ({
      onCockpit,
      onResult,
      onMutated,
      onDismiss,
      isMounted: () => mountedRef.current,
    }),
    [onCockpit, onDismiss, onMutated, onResult],
  );

  const label = (action: WebAction): string => {
    if (runningAction === action.action) return `${action.label} …`;
    return action.label;
  };

  const run = async (action: WebAction, confirmed: boolean) => {
    if (mountedRef.current) setRunningAction(action.action);
    const interactionId = interactionRef.current;
    try {
      await runTaskAction(action, handle, confirmed, interactionId, mutationCallbacks());
      if (interactionId) interactionRef.current = null;
    } finally {
      if (mountedRef.current) setRunningAction(null);
    }
  };

  const handleClick = (action: WebAction) => {
    if (runningAction) return;
    // Confirm toast is non-modal; block sibling ActionBar posts while it is open.
    // Re-tapping the same armed action keeps the first confirm (no re-arm).
    if (pendingConfirmAction !== null) {
      if (action.action !== pendingConfirmAction) onCancelPendingConfirm?.();
      return;
    }
    const needsConfirm = action.destructive || action.confirmation_required;
    if (needsConfirm) {
      const interactionId = beginInteraction(action.action);
      endTapToFeedback(interactionId, "confirm");
      onResult?.(`Confirm ${action.label} for ${handle}?`, null, false, {
        pendingConfirm: { action, handle, interactionId },
      });
      return;
    }
    const interactionId = beginInteraction(action.action);
    interactionRef.current = interactionId;
    void run(action, false);
  };

  return (
    <div className="action-row" style={actionRowStyle}>
      {actions.map((action, index) => (
        <button
          key={action.action}
          type="button"
          className={actionClassName(action, index, runningAction)}
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
