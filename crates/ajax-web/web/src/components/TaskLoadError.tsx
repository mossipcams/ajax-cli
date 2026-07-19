export default function TaskLoadError({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div data-testid="task-load-error">
      <p className="empty">Could not load this task — {message}</p>
      <button type="button" className="pill" onClick={onRetry}>
        Retry
      </button>
    </div>
  );
}
