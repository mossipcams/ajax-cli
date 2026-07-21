import { useEffect, useRef } from "react";
import { DROP_UNDO_MS, RESULT_AUTO_DISMISS_MS, RESULT_SUCCESS_DISMISS_MS } from "@/shared/lib/polling";
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
}

export default function ResultPanel({
  message,
  output = null,
  isError = false,
  onDismiss,
  onUndo,
  onCommit,
}: Props) {
  const trimmedOutput = output?.trim() || null;
  const undoArmed = !!onUndo || !!onCommit;
  const onDismissRef = useRef(onDismiss);
  const onUndoRef = useRef(onUndo);
  const onCommitRef = useRef(onCommit);
  onDismissRef.current = onDismiss;
  onUndoRef.current = onUndo;
  onCommitRef.current = onCommit;

  useEffect(() => {
    const dismissMs = undoArmed
      ? DROP_UNDO_MS
      : isError
        ? RESULT_AUTO_DISMISS_MS
        : RESULT_SUCCESS_DISMISS_MS;
    const timer = setTimeout(() => {
      if (undoArmed) onCommitRef.current?.();
      onDismissRef.current?.();
    }, dismissMs);
    return () => clearTimeout(timer);
  }, [message, undoArmed, isError]);

  function dismiss() {
    if (undoArmed) onUndoRef.current?.();
    onDismissRef.current?.();
  }

  return (
    <div
      className={`result-panel${isError ? " is-error" : ""}`}
      role={isError ? "alert" : "status"}
      aria-live={isError ? "assertive" : "polite"}
    >
      <p className="result-message">{message}</p>
      {trimmedOutput ? <pre className="result-output">{trimmedOutput}</pre> : null}
      {undoArmed ? (
        <Button type="button" variant="default" onClick={dismiss}>
          Undo
        </Button>
      ) : null}
      <Button type="button" variant="secondary" onClick={dismiss}>
        Dismiss
      </Button>
    </div>
  );
}
