import type { BrowserBackend, ConnectionState } from "@/shared/lib/types";

// The last band on the dashboard: is the thing driving all of this healthy?
// Everything here is server-reported or transport-observed — never inferred.

interface Props {
  backend: BrowserBackend;
  connection: ConnectionState;
  taskCount: number;
  onOpenSettings?: () => void;
}

/** Connection words map straight through except the two that read as jargon in
 * a status list. `connected` is the quiet case and carries the ok tone. */
const CONNECTION_TONE: Record<ConnectionState, string> = {
  connected: "success",
  checking: "muted",
  reconnecting: "waiting",
  disconnected: "error",
  "backend unreachable": "error",
  "stale session": "waiting",
};

/** Sentence-case the state for a status list. The raw lowercase word stays the
 * contract elsewhere (ConnectionStatus, `data-state`); this is presentation. */
function stateLabel(connection: ConnectionState): string {
  return connection.charAt(0).toUpperCase() + connection.slice(1);
}

export default function SystemPanel({ backend, connection, taskCount, onOpenSettings }: Props) {
  const controlTone = backend.control_enabled ? "success" : "waiting";
  const controlLabel = backend.control_enabled ? "Control enabled" : "Read-only";

  return (
    <section className="system-panel" aria-label="System status">
      <div className="task-band-title">
        <span className="task-band-label">System</span>
      </div>

      {backend.warning ? (
        <p className="system-warning" data-testid="system-warning">
          {backend.warning}
        </p>
      ) : null}

      <dl className="system-grid">
        <div>
          <dt>Link</dt>
          <dd className={`tone-${CONNECTION_TONE[connection] ?? "muted"}`} data-testid="system-link">
            <span className="system-dot" aria-hidden="true" />
            {stateLabel(connection)}
          </dd>
        </div>
        <div>
          <dt>Control</dt>
          <dd className={`tone-${controlTone}`} data-testid="system-control">
            <span className="system-dot" aria-hidden="true" />
            {controlLabel}
          </dd>
        </div>
        <div>
          <dt>Authority</dt>
          <dd className="tone-muted">{backend.authority}</dd>
        </div>
        <div>
          <dt>Tasks</dt>
          <dd className="tone-muted">{taskCount}</dd>
        </div>
      </dl>

      <button type="button" className="pill system-settings" onClick={() => onOpenSettings?.()}>
        Diagnostics
      </button>
    </section>
  );
}
