import { useEffect, useRef } from "react";
import {
  CONFIRM_TIMEOUT_MS,
  DROP_UNDO_MS,
  RESULT_AUTO_DISMISS_MS,
  RESULT_SUCCESS_DISMISS_MS,
} from "@/shared/lib/polling";
import { Button } from "./button";

interface Props {
  message: string;
  output?: string | null;
  isError?: boolean;
  onDismiss?: () => void;
  /** Cancel a pending pre-commit action (e.g. delayed Drop). */
  onUndo?: () => void;
  /** Commit a pending pre-commit action when the undo window elapses. */
  onCommit?: () => void;
  /** Shell confirm: primary proceeds into the mutation path. */
  onConfirm?: () => void;
  /** Shell confirm: operator cancelled (not timeout). */
  onCancelConfirm?: () => void;
  /** Shell confirm: timeout fired — parent emits confirm_timeout telemetry. */
  onConfirmTimeout?: () => void;
  confirmTimeoutMs?: number;
}

export default function ResultPanel({
  message,
  output = null,
  isError = false,
  onDismiss,
  onUndo,
  onCommit,
  onConfirm,
  onCancelConfirm,
  onConfirmTimeout,
  confirmTimeoutMs = CONFIRM_TIMEOUT_MS,
}: Props) {
  const trimmedOutput = output?.trim() || null;
  const confirmMode = !!onConfirm;
  const undoArmed = !confirmMode && (!!onUndo || !!onCommit);
  const onDismissRef = useRef(onDismiss);
  const onUndoRef = useRef(onUndo);
  const onCommitRef = useRef(onCommit);
  const onConfirmRef = useRef(onConfirm);
  const onCancelConfirmRef = useRef(onCancelConfirm);
  const onConfirmTimeoutRef = useRef(onConfirmTimeout);
  const confirmLatchRef = useRef(false);
  onDismissRef.current = onDismiss;
  onUndoRef.current = onUndo;
  onCommitRef.current = onCommit;
  onConfirmRef.current = onConfirm;
  onCancelConfirmRef.current = onCancelConfirm;
  onConfirmTimeoutRef.current = onConfirmTimeout;

  useEffect(() => {
    confirmLatchRef.current = false;
  }, [message, confirmMode]);

  useEffect(() => {
    const dismissMs = confirmMode
      ? confirmTimeoutMs
      : undoArmed
        ? DROP_UNDO_MS
        : isError
          ? RESULT_AUTO_DISMISS_MS
          : RESULT_SUCCESS_DISMISS_MS;
    const timer = setTimeout(() => {
      if (confirmMode) {
        onConfirmTimeoutRef.current?.();
        onDismissRef.current?.();
        return;
      }
      if (undoArmed) onCommitRef.current?.();
      onDismissRef.current?.();
    }, dismissMs);
    return () => clearTimeout(timer);
  }, [message, confirmMode, undoArmed, isError, confirmTimeoutMs]);

  function dismiss() {
    if (undoArmed) onUndoRef.current?.();
    onDismissRef.current?.();
  }

  function cancelConfirm() {
    if (confirmLatchRef.current) return;
    confirmLatchRef.current = true;
    onCancelConfirmRef.current?.();
    onDismissRef.current?.();
  }

  function confirm() {
    if (confirmLatchRef.current) return;
    confirmLatchRef.current = true;
    onConfirmRef.current?.();
    onDismissRef.current?.();
  }

  return (
    <div
      className={`result-panel${isError ? " is-error" : ""}`}
      role={isError ? "alert" : "status"}
      aria-live={isError ? "assertive" : "polite"}
      data-testid={confirmMode ? "result-panel-confirm" : "result-panel"}
    >
      <p className="result-message">{message}</p>
      {trimmedOutput ? <pre className="result-output">{trimmedOutput}</pre> : null}
      <div className="result-actions" data-testid="result-actions">
        {confirmMode ? (
          <>
            <Button type="button" variant="default" onClick={confirm}>
              Confirm
            </Button>
            <Button type="button" variant="secondary" onClick={cancelConfirm}>
              Cancel
            </Button>
          </>
        ) : undoArmed ? (
          <Button type="button" variant="default" onClick={dismiss}>
            Undo
          </Button>
        ) : null}
        {!confirmMode ? (
          <Button type="button" variant="secondary" onClick={dismiss}>
            Dismiss
          </Button>
        ) : null}
      </div>
    </div>
  );
}
