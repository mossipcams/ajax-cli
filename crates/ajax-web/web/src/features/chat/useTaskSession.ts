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
import {
  modelLiveOption,
  readLiveSelectCurrent,
} from "@/shared/lib/liveSessionConfig";
import { DEFAULT_SESSION_MODEL, writeSessionModel } from "@/features/task/public";
import { initialSessionState, sessionReducer } from "./sessionThread";
import { useSessionTransport } from "./useSessionTransport";
import { isSessionConfigChangeFailure } from "./sessionModel";

interface Options {
  handle: string | null;
  detail: BrowserTaskDetail | null;
  onMutated?: () => void;
  onConfigError?: (message: string) => void;
}

function modelFromOptions(options: LiveSessionConfigOption[] | undefined): string {
  if (!options?.length) return DEFAULT_SESSION_MODEL;
  const model = modelLiveOption(options);
  return model ? readLiveSelectCurrent(model) ?? DEFAULT_SESSION_MODEL : DEFAULT_SESSION_MODEL;
}

export function useTaskSession({ handle, detail, onMutated, onConfigError }: Options) {
  const [state, dispatch] = useReducer(sessionReducer, initialSessionState);
  const [connected, setConnected] = useState(false);
  const [everOpened, setEverOpened] = useState(false);
  const [activityAgeMs, setActivityAgeMs] = useState(0);
  const [sessionModel, setSessionModel] = useState(DEFAULT_SESSION_MODEL);
  const [sessionConfigOptions, setSessionConfigOptions] = useState<
    LiveSessionConfigOption[] | undefined
  >(undefined);

  const transportRef = useRef<WebSessionTransport | undefined>(undefined);
  const connectedRef = useRef(false);
  const everOpenedRef = useRef(false);
  const detailRef = useRef(detail);
  const lastActivityAtRef = useRef(Date.now());
  const onConfigErrorRef = useRef(onConfigError);
  onConfigErrorRef.current = onConfigError;

  useEffect(() => {
    setSessionModel(DEFAULT_SESSION_MODEL);
    setSessionConfigOptions(undefined);
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
    setSessionModel(next);
    writeSessionModel(next);
  }, []);

  const applyHostConfigOptions = useCallback((options: LiveSessionConfigOption[] | undefined) => {
    setSessionConfigOptions(options);
    const next = modelFromOptions(options);
    setSessionModel(next);
    if (options?.length) writeSessionModel(next);
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
    onSessionConfigOptions: applyHostConfigOptions,
    onSessionModelRejected: () => {},
    onConfigError: (message) => onConfigErrorRef.current?.(message),
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

  const markStopped = useCallback(() => {
    dispatch({ type: "event", event: { type: "message", role: "system", text: "Stopped" } });
  }, []);

  const applyConfigOption = useCallback((configId: string, value: string | boolean) => {
    transportRef.current?.setConfigOption(configId, value);
  }, []);

  const applyModel = useCallback((catalogId: string) => {
    transportRef.current?.setModel(catalogId);
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
    applyConfigOption,
    applyModel,
    respondPermission,
    onMutated: handleMutated,
  };
}
