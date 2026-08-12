import { startTransition, useCallback, useEffect, useRef, useState } from "react";
import { ApiError, fetchCockpit } from "@/shared/lib/api";
import {
  createCockpitApplyGate,
  createInFlightGuard,
  gestureBusyGate,
} from "@/shared/lib/cockpitPoll";
import type { BrowserCockpitView, ConnectionState, RemoteResource } from "@/shared/lib/types";

export type LoadCockpitOptions = {
  /** Schedule a follow-up poll if one is already in flight (Retry). */
  trailing?: boolean;
  /** Interval poll only — resume/recovery loads must not use this blindly. */
  deferDuringGesture?: boolean;
};

export type CockpitResource = {
  cockpit: RemoteResource<BrowserCockpitView>;
  connection: ConnectionState;
  connectionDetail: string | null;
  loadCockpit: (options?: LoadCockpitOptions) => Promise<void>;
  applyCockpit: (next: BrowserCockpitView) => void;
  applyConnectionError: (error: unknown) => void;
  /**
   * Mark the connection healthy without touching the cockpit projection.
   * Non-cockpit successes (a task-detail load) need to clear the error banner
   * but must not re-apply cockpit data to do it.
   */
  markConnected: () => void;
};

function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  const message = error instanceof Error ? error.message : String(error);
  return new ApiError("network", message);
}

export function useCockpitResource(): CockpitResource {
  const [cockpit, setCockpit] = useState<RemoteResource<BrowserCockpitView>>({
    status: "loading",
    data: null,
    error: null,
  });
  const [connection, setConnection] = useState<ConnectionState>("checking");
  const [connectionDetail, setConnectionDetail] = useState<string | null>(null);

  const cockpitApplyGateRef = useRef(createCockpitApplyGate());
  const cockpitPollGuardRef = useRef(createInFlightGuard());
  const deferredPollRef = useRef<{ view: BrowserCockpitView; startedAt: number } | null>(null);

  const commitMutationProjection = useCallback((next: BrowserCockpitView) => {
    const gate = cockpitApplyGateRef.current;
    gate.noteMutation();
    const projectionChanged = gate.applyIfChanged(next);
    startTransition(() => {
      if (projectionChanged) {
        setCockpit({ status: "ready", data: next, error: null });
      } else {
        setCockpit((prev) => {
          if (prev.status === "stale") {
            return { status: "ready", data: prev.data, error: null };
          }
          return prev;
        });
      }
      setConnection("connected");
      setConnectionDetail(null);
    });
  }, []);

  const commitPollProjection = useCallback((next: BrowserCockpitView, startedAt: number) => {
    const gate = cockpitApplyGateRef.current;
    if (startedAt !== gate.pollGeneration()) return;
    const projectionChanged = gate.applyPollIfChanged(next, startedAt);
    startTransition(() => {
      if (projectionChanged) {
        setCockpit({ status: "ready", data: next, error: null });
      } else {
        setCockpit((prev) => {
          if (prev.status === "stale") {
            return { status: "ready", data: prev.data, error: null };
          }
          return prev;
        });
      }
      setConnection("connected");
      setConnectionDetail(null);
    });
  }, []);

  useEffect(() => {
    return gestureBusyGate.onIdle(() => {
      const deferred = deferredPollRef.current;
      if (!deferred) return;
      deferredPollRef.current = null;
      commitPollProjection(deferred.view, deferred.startedAt);
    });
  }, [commitPollProjection]);

  const applyConnectionError = useCallback((error: unknown) => {
    if (error instanceof ApiError) {
      setConnection(
        error.kind === "network"
          ? "backend unreachable"
          : error.kind === "stale-session"
            ? "stale session"
            : "disconnected",
      );
      setConnectionDetail(error.message);
      return;
    }
    setConnection("backend unreachable");
    setConnectionDetail(error instanceof Error ? error.message : String(error));
  }, []);

  const markConnected = useCallback(() => {
    setConnection("connected");
    setConnectionDetail(null);
  }, []);

  const applyCockpit = useCallback(
    (next: BrowserCockpitView) => {
      deferredPollRef.current = null;
      commitMutationProjection(next);
    },
    [commitMutationProjection],
  );

  // No document.hidden guard here: an iOS home-screen PWA mounts while the
  // splash screen still reports the document hidden, and swallowing the mount
  // load stranded the app on "checking" until the (60s, hidden) interval fired.
  // Skipping while hidden is a *background poll* concern — see App.tsx.
  const loadCockpit = useCallback(async (options?: LoadCockpitOptions) => {
    await cockpitPollGuardRef.current.run(
      async () => {
        try {
          const startedAt = cockpitApplyGateRef.current.pollGeneration();
          const next = await fetchCockpit();
          if (options?.deferDuringGesture && gestureBusyGate.isBusy()) {
            deferredPollRef.current = { view: next, startedAt };
            return;
          }
          commitPollProjection(next, startedAt);
        } catch (error) {
          applyConnectionError(error);
          const apiError = toApiError(error);
          setCockpit((prev) => {
            if (prev.status === "ready" || prev.status === "stale") {
              return { status: "stale", data: prev.data, error: apiError };
            }
            return { status: "error", data: null, error: apiError };
          });
        }
      },
      options?.trailing ? { trailing: true } : undefined,
    );
  }, [commitPollProjection, applyConnectionError]);

  return {
    cockpit,
    connection,
    connectionDetail,
    loadCockpit,
    applyCockpit,
    applyConnectionError,
    markConnected,
  };
}
