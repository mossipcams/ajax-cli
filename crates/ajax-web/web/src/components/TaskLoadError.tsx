import { Button } from "./ui/button";

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
      <Button type="button" variant="secondary" onClick={onRetry}>
        Retry
      </Button>
    </div>
  );
}
