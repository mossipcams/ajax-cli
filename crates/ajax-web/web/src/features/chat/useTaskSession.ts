import {
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
  type MutableRefObject,
} from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import {
  clearSessionTransportState,
  type WebSessionTransport,
} from "@/shared/lib/webSessionTransport";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import { DEFAULT_SESSION_MODEL, writeSessionModel } from "@/features/task/public";
import { initialSessionState, sessionReducer } from "./sessionThread";
import { useSessionTransport } from "./useSessionTransport";

interface Options {
  handle: string | null;
  detail: BrowserTaskDetail | null;
  onMutated?: () => void;
}

export function useTaskSession({ handle, detail, onMutated }: Options) {
  const [state, dispatch] = useReducer(sessionReducer, initialSessionState);
  const [connected, setConnected] = useState(false);
  const [everOpened, setEverOpened] = useState(false);
  const [activityAgeMs, setActivityAgeMs] = useState(0);
  /** Host-authoritative model for this task's live session (not localStorage). */
  const [sessionModel, setSessionModel] = useState(DEFAULT_SESSION_MODEL);
  const [sessionConfigOptions, setSessionConfigOptions] = useState<
    LiveSessionConfigOption[] | undefined
  >(undefined);
  const serverModelRef = useRef(DEFAULT_SESSION_MODEL);
  const pendingModelRef = useRef<string | null>(null);

  const transportRef = useRef<WebSessionTransport | undefined>(undefined);
  const connectedRef = useRef(false);
  const everOpenedRef = useRef(false);
  const detailRef = useRef(detail);
  const lastActivityAtRef = useRef(Date.now());

  useEffect(() => {
    setSessionModel(DEFAULT_SESSION_MODEL);
    setSessionConfigOptions(undefined);
    serverModelRef.current = DEFAULT_SESSION_MODEL;
    pendingModelRef.current = null;
  }, [handle]);

  detailRef.current = detail;
  connectedRef.current = connected;

  const markActivity = useCallback(() => {
    lastActivityAtRef.current = Date.now();
    setActivityAgeMs(0);
  }, []);

  const invalidateSession = useCallback(() => {
    if (handle) clearSessionTransportState(handle);
  }, [handle]);

  const applyHostSessionModel = useCallback((nextModel: string) => {
    const next = nextModel.trim() || DEFAULT_SESSION_MODEL;
    if (pendingModelRef.current !== null && pendingModelRef.current !== next) {
      return;
    }
    pendingModelRef.current = null;
    serverModelRef.current = next;
    setSessionModel(next);
    // Seed the New Task picker only; task metadata remains authoritative in-session.
    writeSessionModel(next);
  }, []);

  const revertPendingModelChange = useCallback(() => {
    if (pendingModelRef.current === null) return;
    pendingModelRef.current = null;
    setSessionModel(serverModelRef.current);
  }, []);

  useSessionTransport({
    handle,
    dispatch,
    detailRef,
    transportRef,
    connectedRef,
    everOpenedRef,
    onActivity: markActivity,
    setConnected,
    setEverOpened,
    onSessionInvalidated: invalidateSession,
    onSessionModel: applyHostSessionModel,
    onSessionConfigOptions: setSessionConfigOptions,
    onSessionModelRejected: revertPendingModelChange,
  });

  useEffect(() => {
    if (!state.busy) return;
    const timer = window.setInterval(
      () => setActivityAgeMs(Date.now() - lastActivityAtRef.current),
      30_000,
    );
    return () => window.clearInterval(timer);
  }, [state.busy]);

  const sendPrompt = useCallback(
    (text: string): boolean => {
      const trimmed = text.trim();
      if (!trimmed || !connected) return false;
      if (!transportRef.current?.sendPrompt(trimmed)) return false;
      if (!state.busy) markActivity();
      dispatch({ type: "prompt", text: trimmed });
      return true;
    },
    [connected, markActivity, state.busy],
  );

  const sendCancel = useCallback(() => {
    transportRef.current?.sendCancel();
  }, []);

  /** A stop the operator asked for is session history, not an ACP event: the
   * host has no update that says "the human interrupted here". */
  const markStopped = useCallback(() => {
    dispatch({ type: "event", event: { type: "message", role: "system", text: "Stopped" } });
  }, []);

  const setModel = useCallback((model: string) => {
    const trimmed = model.trim() || DEFAULT_SESSION_MODEL;
    pendingModelRef.current = trimmed;
    setSessionModel(trimmed);
    transportRef.current?.setModel(trimmed);
  }, []);

  const respondPermission = useCallback(
    (approved: boolean) => {
      const decision = state.decision;
      if (!decision || !connected) return;
      transportRef.current?.respondPermission(decision.requestId, approved);
    },
    [connected, state.decision],
  );

  const handleMutated = useCallback(() => {
    if (handle) clearSessionTransportState(handle);
    onMutated?.();
  }, [handle, onMutated]);

  return {
    state,
    connected,
    everOpened,
    activityAgeMs,
    sessionModel,
    sessionConfigOptions,
    transportRef: transportRef as MutableRefObject<WebSessionTransport | undefined>,
    sendPrompt,
    sendCancel,
    markStopped,
    setModel,
    respondPermission,
    onMutated: handleMutated,
  };
}
