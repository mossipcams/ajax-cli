import { useEffect, useRef } from "react";
import { Button } from "@/shared/ui/button";

export default function TaskLoadError({
  message,
  onBack,
  onRetry,
}: {
  message: string;
  onBack: () => void;
  onRetry: () => void;
}) {
  const retryLatchRef = useRef(false);

  useEffect(() => {
    retryLatchRef.current = false;
  }, [message]);

  return (
    <div data-testid="task-load-error">
      <p className="empty">Could not load this task — {message}</p>
      <Button type="button" variant="secondary" onClick={onBack}>
        ← Back
      </Button>
      <Button
        type="button"
        variant="secondary"
        onClick={() => {
          if (retryLatchRef.current) return;
          retryLatchRef.current = true;
          onRetry();
        }}
      >
        Retry
      </Button>
    </div>
  );
}
