import { useEffect, useRef, useState } from "react";
import { Button } from "@/shared/ui/button";
import { ApiError, fetchDevDeploy, startDevDeploy } from "@/shared/lib/api";
import type { DevDeployStatus } from "@/shared/lib/types";

interface Props {
  taskHandle: string;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
}

export default function TestInDevPanel({ taskHandle, onResult }: Props) {
  const [status, setStatus] = useState<DevDeployStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const pollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  async function refresh() {
    try {
      const response = await fetchDevDeploy();
      setStatus(response.deploy);
      if (!response.deploy.active && pollTimerRef.current) {
        clearInterval(pollTimerRef.current);
        pollTimerRef.current = null;
      }
    } catch {
      // Keep last known status; transient network blips during restart are expected.
    }
  }

  function startPolling() {
    if (pollTimerRef.current) return;
    pollTimerRef.current = setInterval(() => {
      void refresh();
    }, 1500);
  }

  async function deploy() {
    if (busy || status?.active) return;
    setBusy(true);
    try {
      const response = await startDevDeploy(taskHandle);
      setStatus(response.deploy);
      startPolling();
      onResult?.("Test in Dev started", null, false);
    } catch (error) {
      const message =
        error instanceof ApiError ? error.message : "Test in Dev failed to start";
      onResult?.(message, null, true);
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void refresh();
    return () => {
      if (pollTimerRef.current) clearInterval(pollTimerRef.current);
    };
  }, []);

  const phaseLabel = status?.phase_label ?? "Ready to deploy";
  const disabled = busy || !!status?.active;
  const error = status?.error ?? null;

  return (
    <section className="test-in-dev" data-testid="test-in-dev" aria-label="Test in Dev">
      <div className="test-in-dev-row">
        <div className="actions">
          <Button
            type="button"
            variant="secondary"
            data-testid="test-in-dev-button"
            disabled={disabled}
            onClick={() => void deploy()}
          >
            {disabled ? `${phaseLabel}…` : "Test in Dev"}
          </Button>
        </div>
      </div>
      {error ? (
        <pre className="error" data-testid="test-in-dev-error">
          {error}
        </pre>
      ) : null}
    </section>
  );
}
