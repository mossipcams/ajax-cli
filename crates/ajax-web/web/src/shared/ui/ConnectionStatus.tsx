import { useEffect, useRef, useState } from "react";
import type { ConnectionState } from "@/shared/lib/types";

export const ERROR_HOLD_MS = 4000;

const HOLDABLE_ERROR_STATES: readonly ConnectionState[] = [
  "disconnected",
  "backend unreachable",
  "stale session",
];

interface Props {
  state: ConnectionState;
  detail?: string | null;
  healthHref?: string;
  onRetry?: () => void;
  onReload?: () => void;
  onCopyDiagnostics?: () => void;
  onSignIn?: () => void;
  holdErrors?: boolean;
}

export default function ConnectionStatus({
  state,
  detail = null,
  healthHref = "/api/health",
  onRetry,
  onReload,
  onCopyDiagnostics,
  onSignIn,
  holdErrors = false,
}: Props) {
  const isHoldableError = HOLDABLE_ERROR_STATES.includes(state);
  const [releasedFor, setReleasedFor] = useState<ConnectionState | null>(null);

  if (!isHoldableError && releasedFor !== null) {
    setReleasedFor(null);
  }

  const held = holdErrors && isHoldableError && releasedFor !== state;

  useEffect(() => {
    if (!held) return;
    const timer = setTimeout(() => setReleasedFor(state), ERROR_HOLD_MS);
    return () => clearTimeout(timer);
  }, [held, state]);

  const displayed: ConnectionState = held ? "reconnecting" : state;
  const label = detail ? `${displayed}: ${detail}` : displayed;
  const retryLatchRef = useRef(false);
  const reloadLatchRef = useRef(false);

  if (state === "sign-in expired") {
    return (
      <div className="connection-status" data-testid="connection-status" data-state={displayed}>
        <div className="connection-actions" aria-label="Connection actions">
          <button type="button" className="is-primary" onClick={() => onSignIn?.()}>
            Sign-in expired. Tap to sign in.
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="connection-status" data-testid="connection-status" data-state={displayed}>
      <span className="connection-label">{label}</span>
      <div className="connection-actions" aria-label="Connection actions">
        <button
          type="button"
          className="is-primary"
          onClick={() => {
            if (retryLatchRef.current) return;
            retryLatchRef.current = true;
            onRetry?.();
            queueMicrotask(() => {
              retryLatchRef.current = false;
            });
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
            queueMicrotask(() => {
              reloadLatchRef.current = false;
            });
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
