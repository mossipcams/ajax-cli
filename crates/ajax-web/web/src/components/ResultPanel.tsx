import { useEffect } from "react";
import { DROP_UNDO_MS, RESULT_AUTO_DISMISS_MS, RESULT_SUCCESS_DISMISS_MS } from "../polling";

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

  useEffect(() => {
    const dismissMs = undoArmed
      ? DROP_UNDO_MS
      : isError
        ? RESULT_AUTO_DISMISS_MS
        : RESULT_SUCCESS_DISMISS_MS;
    const timer = setTimeout(() => {
      if (undoArmed) onCommit?.();
      onDismiss?.();
    }, dismissMs);
    return () => clearTimeout(timer);
  }, [message, undoArmed, isError, onCommit, onDismiss]);

  function dismiss() {
    if (undoArmed) onUndo?.();
    onDismiss?.();
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
        <button type="button" className="pill is-primary" onClick={dismiss}>
          Undo
        </button>
      ) : null}
      <button type="button" className="pill" onClick={dismiss}>
        Dismiss
      </button>
    </div>
  );
}
