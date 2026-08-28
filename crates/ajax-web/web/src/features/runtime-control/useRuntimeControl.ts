import { useCallback, useEffect, useRef, useState } from "react";
import {
  ApiError,
  fetchRuntimeStatus,
  restartServer,
  updateServer,
} from "@/shared/lib/api";
import { CONFIRM_TIMEOUT_MS, RUNTIME_STATUS_POLL_MS } from "@/shared/lib/polling";
import type { RuntimeStatusResponse } from "@/shared/lib/types";
import { isTerminalRuntimeResult, waitForRuntimeOperationResult } from "./reconnect";

function operationOverlayLabel(
  action: "restart" | "update",
  status: RuntimeStatusResponse | null,
): string {
  const phase = status?.operation?.phase;
  if (phase && phase !== "queued") {
    const kind = status?.operation?.kind ?? action;
    return `${kind} · ${phase}`;
  }
  return action === "restart" ? "Restarting Ajax…" : "Updating Ajax…";
}

export function useRuntimeControl() {
  const [status, setStatus] = useState<RuntimeStatusResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [overlay, setOverlay] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmAction, setConfirmAction] = useState<"restart" | "update" | null>(null);
  const confirmTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await fetchRuntimeStatus();
      setStatus(next);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => {
      void refresh();
    }, RUNTIME_STATUS_POLL_MS * 3);
    return () => window.clearInterval(timer);
  }, [refresh]);

  function beginConfirm(action: "restart" | "update") {
    setConfirmAction(action);
    if (confirmTimer.current) clearTimeout(confirmTimer.current);
    confirmTimer.current = setTimeout(() => setConfirmAction(null), CONFIRM_TIMEOUT_MS);
  }

  async function runOperation(action: "restart" | "update") {
    if (confirmAction !== action) {
      beginConfirm(action);
      return;
    }
    if (confirmTimer.current) clearTimeout(confirmTimer.current);
    setConfirmAction(null);
    setBusy(true);
    setError(null);
    const previousVersion = status?.version ?? null;
    let restarting = true;
    try {
      const response =
        action === "restart" ? await restartServer() : await updateServer();
      restarting = response.restarting !== false;
    } catch (caught) {
      const message =
        caught instanceof ApiError
          ? caught.message
          : caught instanceof Error
            ? caught.message
            : "Operation failed";
      setError(message);
      setBusy(false);
      return;
    }

    if (restarting) {
      setOverlay(action === "restart" ? "Restarting Ajax…" : "Updating Ajax…");
    } else {
      setOverlay(null);
    }

    const outcome = await waitForRuntimeOperationResult({
      previousVersion,
      restarting,
      onStatus: (next) => {
        setStatus(next);
        if (restarting) {
          setOverlay(operationOverlayLabel(action, next));
        }
      },
    });
    setOverlay(null);
    setBusy(false);
    if (outcome.status) {
      setStatus(outcome.status);
    } else {
      await refresh();
    }
  }

  const updateAvailable =
    typeof status?.update_available === "object" && status.update_available !== null
      ? status.update_available.available === true
      : status?.update_available === true;

  const operationLabel = status?.operation
    ? `${status.operation.kind} · ${status.operation.phase}`
    : "idle";

  const terminalResult = isTerminalRuntimeResult(status);

  return {
    status,
    loading,
    busy,
    overlay,
    error,
    dismissError: () => setError(null),
    confirmAction,
    updateAvailable,
    operationLabel,
    terminalResult,
    refresh,
    runRestart: () => void runOperation("restart"),
    runUpdate: () => void runOperation("update"),
  };
}
