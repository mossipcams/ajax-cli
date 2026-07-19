import type { ConnectionState } from "@/shared/lib/types";

interface Props {
  state: ConnectionState;
  detail?: string | null;
  healthHref?: string;
  onRetry?: () => void;
  onReload?: () => void;
  onCopyDiagnostics?: () => void;
}

export default function ConnectionStatus({
  state,
  detail = null,
  healthHref = "/api/health",
  onRetry,
  onReload,
  onCopyDiagnostics,
}: Props) {
  const label = detail ? `${state}: ${detail}` : state;

  return (
    <div className="connection-status" data-testid="connection-status" data-state={state}>
      <span className="connection-label">{label}</span>
      <div className="connection-actions" aria-label="Connection actions">
        <button type="button" className="is-primary" onClick={() => onRetry?.()}>
          Retry
        </button>
        <button type="button" onClick={() => onReload?.()}>
          Reload
        </button>
        <button type="button" onClick={() => onCopyDiagnostics?.()}>
          Copy Diagnostics
        </button>
        <a href={healthHref} target="_blank" rel="noreferrer">
          Open Health URL
        </a>
      </div>
    </div>
  );
}
