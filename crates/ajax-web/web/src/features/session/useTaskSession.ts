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
import { useSessionModelPreference } from "./sessionModel";
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
  const [sessionModel, setSessionModel] = useSessionModelPreference();

  const transportRef = useRef<WebSessionTransport | undefined>(undefined);
  const connectedRef = useRef(false);
  const everOpenedRef = useRef(false);
  const detailRef = useRef(detail);
  const lastActivityAtRef = useRef(Date.now());

  detailRef.current = detail;
  connectedRef.current = connected;

  const markActivity = useCallback(() => {
    lastActivityAtRef.current = Date.now();
    setActivityAgeMs(0);
  }, []);

  const invalidateSession = useCallback(() => {
    if (handle) clearSessionTransportState(handle);
  }, [handle]);

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

  const setModel = useCallback(
    (model: string) => {
      setSessionModel(model);
      transportRef.current?.setModel(model);
    },
    [setSessionModel],
  );

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
    transportRef: transportRef as MutableRefObject<WebSessionTransport | undefined>,
    sendPrompt,
    sendCancel,
    setModel,
    respondPermission,
    onMutated: handleMutated,
  };
}
