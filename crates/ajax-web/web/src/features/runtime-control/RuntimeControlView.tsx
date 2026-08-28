import { Button } from "@/shared/ui/button";
import { useRuntimeControl } from "./useRuntimeControl";

interface Props {
  onBack?: () => void;
}

function formatUptime(seconds?: number): string {
  if (seconds == null) return "—";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remMinutes = minutes % 60;
  return remMinutes > 0 ? `${hours}h ${remMinutes}m` : `${hours}h`;
}

export default function RuntimeControlView({ onBack }: Props) {
  const {
    status,
    loading,
    busy,
    overlay,
    error,
    dismissError,
    confirmAction,
    updateAvailable,
    operationLabel,
    terminalResult,
    runRestart,
    runUpdate,
  } = useRuntimeControl();

  const logs = status?.logs ?? [];

  return (
    <section className="runtime-control-view" aria-labelledby="runtime-control-heading">
      <div className="runtime-control-header">
        <Button type="button" variant="secondary" onClick={() => onBack?.()}>
          Back
        </Button>
        <h2 id="runtime-control-heading">Control</h2>
      </div>

      <div className="runtime-control-section" data-testid="runtime-control-status">
        <h3>Server status</h3>
        {loading ? <p className="runtime-control-note">Loading…</p> : null}
        <dl className="runtime-control-dl">
          <div>
            <dt>Health</dt>
            <dd>{status?.ok ? "ok" : "unknown"}</dd>
          </div>
          <div>
            <dt>Version</dt>
            <dd>{status?.version ?? "—"}</dd>
          </div>
          <div>
            <dt>Commit</dt>
            <dd>{status?.commit ?? "unknown"}</dd>
          </div>
          <div>
            <dt>Profile</dt>
            <dd>{status?.profile ?? "—"}</dd>
          </div>
          <div>
            <dt>Uptime</dt>
            <dd>{formatUptime(status?.uptime_seconds)}</dd>
          </div>
          <div>
            <dt>Update</dt>
            <dd>{updateAvailable ? "origin/main ahead" : "up to date or unknown"}</dd>
          </div>
          <div>
            <dt>Operation</dt>
            <dd>{operationLabel}</dd>
          </div>
          {terminalResult ? (
            <div>
              <dt>Last result</dt>
              <dd>{terminalResult}</dd>
            </div>
          ) : null}
        </dl>
      </div>

      <div className="runtime-control-section" data-testid="runtime-control-actions">
        <h3>Lifecycle</h3>
        <p className="runtime-control-note">
          Restart relaunches the currently installed control plane only. Update deploys
          origin/main to stable using the existing safe-deploy path.
        </p>
        <div className="runtime-control-actions">
          <Button
            type="button"
            variant="secondary"
            disabled={busy}
            data-testid="runtime-restart"
            onClick={runRestart}
          >
            {confirmAction === "restart" ? "Tap to confirm restart" : "Restart Ajax"}
          </Button>
          <Button
            type="button"
            variant="secondary"
            disabled={busy || !status?.test_in_stable}
            data-testid="runtime-update"
            onClick={runUpdate}
          >
            {confirmAction === "update" ? "Tap to confirm update" : "Update Ajax"}
          </Button>
        </div>
      </div>

      {logs.length > 0 ? (
        <div className="runtime-control-section" data-testid="runtime-control-logs">
          <h3>Recent logs</h3>
          <pre className="runtime-control-log">{logs.join("\n")}</pre>
        </div>
      ) : null}

      {error ? (
        <div
          className="runtime-control-error"
          role="alert"
          data-testid="runtime-control-error"
        >
          <p>{error}</p>
          <Button type="button" variant="secondary" onClick={dismissError}>
            Dismiss
          </Button>
        </div>
      ) : null}

      {overlay ? (
        <div className="runtime-control-overlay" role="status" aria-live="polite">
          <div className="runtime-control-overlay-card">
            <p className="runtime-control-status">{overlay}</p>
            <p className="runtime-control-note">Waiting for the listener to return…</p>
          </div>
        </div>
      ) : null}
    </section>
  );
}
