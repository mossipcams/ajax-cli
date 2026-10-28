import { useEffect, useRef, useState } from "react";
import * as api from "@/shared/lib/api";
import type { CursorAuthStatus, CursorLoginPhase } from "@/shared/lib/api";
import { Button } from "@/shared/ui/button";

const CURSOR_LOGIN_POLL_MS = 2000;

type StatusState =
  | { kind: "loading" }
  | { kind: "error" }
  | { kind: "ready"; status: CursorAuthStatus };

interface Props {
  onSignedIn?: () => void;
}

export default function CursorSignIn({ onSignedIn }: Props) {
  const [status, setStatus] = useState<StatusState>({ kind: "loading" });
  const [phase, setPhase] = useState<CursorLoginPhase>({ phase: "idle" });
  const [starting, setStarting] = useState(false);
  const [active, setActive] = useState(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    let cancelled = false;

    api.fetchCursorAuthStatus()
      .then((value) => {
        if (mountedRef.current && !cancelled) setStatus({ kind: "ready", status: value });
      })
      .catch(() => {
        if (mountedRef.current && !cancelled) setStatus({ kind: "error" });
      });

    api.fetchCursorLoginPhase()
      .then((value) => {
        if (!mountedRef.current || cancelled) return;
        setPhase(value);
        if (value.phase === "waiting" || value.phase === "verifying") setActive(true);
      })
      .catch(() => {
      });

    return () => {
      cancelled = true;
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (!active) return;
    const timer = setInterval(async () => {
      if (!mountedRef.current) return;
      let next: CursorLoginPhase;
      try {
        next = await api.fetchCursorLoginPhase();
      } catch {
        return;
      }
      if (!mountedRef.current) return;
      setPhase(next);
      if (next.phase === "succeeded") {
        setActive(false);
        setStatus({ kind: "loading" });
        try {
          const refreshed = await api.fetchCursorAuthStatus();
          if (mountedRef.current) setStatus({ kind: "ready", status: refreshed });
        } catch {
          if (mountedRef.current) setStatus({ kind: "error" });
        }
        if (mountedRef.current) onSignedIn?.();
      } else if (next.phase === "failed") {
        setActive(false);
      }
    }, CURSOR_LOGIN_POLL_MS);
    return () => clearInterval(timer);
  }, [active, onSignedIn]);

  async function startSignIn() {
    setStarting(true);
    try {
      const started = await api.startCursorLogin();
      if (!mountedRef.current) return;
      setPhase(started);
      if (
        started.phase === "idle" ||
        started.phase === "waiting" ||
        started.phase === "verifying"
      ) {
        setActive(true);
      }
    } catch {
      if (mountedRef.current) setActive(false);
      if (mountedRef.current) setPhase({ phase: "failed", message: "could not start sign-in" });
    } finally {
      if (mountedRef.current) setStarting(false);
    }
  }

  async function cancelSignIn() {
    try {
      const cancelled = await api.cancelCursorLogin();
      if (mountedRef.current) setPhase(cancelled);
    } catch {
    }
  }

  const disabled = starting || active;
  const keyPresent = status.kind === "ready" && status.status.present;

  return (
    <>
      <Button type="button" data-testid="cursor-sign-in" disabled={disabled} onClick={() => void startSignIn()}>
        {keyPresent ? "Sign in again" : "Sign in with Cursor"}
      </Button>
      {phase.phase === "waiting" ? (
        phase.url.startsWith("https://") ? (
          <>
            <a
              data-testid="cursor-sign-in-link"
              href={phase.url}
              target="_blank"
              rel="noopener noreferrer"
            >
              Open Cursor sign-in
            </a>
            <p className="settings-note">Approve the sign-in, then come back here. This updates automatically.</p>
            <Button type="button" variant="secondary" data-testid="cursor-sign-in-cancel" onClick={() => void cancelSignIn()}>
              Cancel
            </Button>
          </>
        ) : (
          <p className="settings-status" data-testid="cursor-sign-in-error">
            Sign-in failed: unexpected sign-in link
          </p>
        )
      ) : null}
      {phase.phase === "verifying" ? <p className="settings-status">Checking the new key…</p> : null}
      {phase.phase === "failed" ? (
        <p className="settings-status" data-testid="cursor-sign-in-error">
          Sign-in failed: {phase.message}
        </p>
      ) : null}
    </>
  );
}
