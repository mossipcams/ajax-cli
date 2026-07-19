import { useCallback, useRef, useState } from "react";
import { ApiError, fetchCockpit } from "@/shared/lib/api";
import { createCockpitApplyGate, createInFlightGuard } from "@/shared/lib/cockpitPoll";
import type { BrowserCockpitView, ConnectionState, RemoteResource } from "@/shared/lib/types";

export type LoadCockpitOptions = {
  /** Schedule a follow-up poll if one is already in flight (Retry). */
  trailing?: boolean;
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

  const applyCockpit = useCallback((next: BrowserCockpitView) => {
    if (cockpitApplyGateRef.current.applyIfChanged(next)) {
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
  }, []);

  const loadCockpit = useCallback(async (options?: LoadCockpitOptions) => {
    if (document.hidden) return;
    await cockpitPollGuardRef.current.run(
      async () => {
        try {
          applyCockpit(await fetchCockpit());
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
  }, [applyCockpit, applyConnectionError]);

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
