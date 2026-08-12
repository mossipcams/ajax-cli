import { useEffect, useRef } from "react";
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
  const retryLatchRef = useRef(false);
  const reloadLatchRef = useRef(false);

  useEffect(() => {
    retryLatchRef.current = false;
  }, [state]);

  return (
    <div className="connection-status" data-testid="connection-status" data-state={state}>
      <span className="connection-label">{label}</span>
      <div className="connection-actions" aria-label="Connection actions">
        <button
          type="button"
          className="is-primary"
          onClick={() => {
            if (retryLatchRef.current) return;
            retryLatchRef.current = true;
            onRetry?.();
          }}
        >
          Retry
        </button>
        <button
          type="button"
          onClick={() => {
            if (reloadLatchRef.current) return;
            reloadLatchRef.current = true;
            onReload?.();
          }}
        >
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
