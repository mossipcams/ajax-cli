import {
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
  type MutableRefObject,
} from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { clearSessionTransportState, type WebSessionTransport } from "./transport/public";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import type { LiveAvailableCommand } from "@/shared/lib/liveSessionCommands";
import type { LivePromptCapabilities } from "@/shared/lib/liveSessionPromptCapabilities";
import type { PromptContentBlockWire } from "@/shared/lib/promptContent";
import {
  modelLiveOption,
  readLiveSelectCurrent,
} from "@/shared/lib/liveSessionConfig";
import { DEFAULT_SESSION_MODEL, writeSessionModel } from "@/features/task/public";
import { initialChatSessionReducerState, reduceChatSession } from "./reducer";
import type { ChatModelState, ChatSessionAction, ChatSessionView } from "./model";
import {
  connectionStateAllowsSend,
  initialConnectionState,
  useSessionConnection,
  type ConnectionState,
} from "./connection/public";

interface Options {
  handle: string | null;
  detail: BrowserTaskDetail | null;
  onMutated?: () => void;
  onConfigError?: (message: string) => void;
}

function modelFromOptions(options: LiveSessionConfigOption[] | undefined): string | undefined {
  if (!options?.length) return undefined;
  const model = modelLiveOption(options);
  return model ? readLiveSelectCurrent(model) : undefined;
}

export function useChatSession({ handle, detail, onMutated, onConfigError }: Options) {
  const [reducerState, dispatch] = useReducer(
    reduceChatSession,
    initialChatSessionReducerState,
  );
  const [connectionState, setConnectionState] = useState<ConnectionState>(initialConnectionState());
  const [everOpened, setEverOpened] = useState(false);
  const [activityAgeMs, setActivityAgeMs] = useState(0);
  const [modelState, setModelState] = useState<ChatModelState>({
    confirmedModel: DEFAULT_SESSION_MODEL,
  });
  const view: ChatSessionView = {
    ...reducerState.view,
    model: modelState,
  };

  const transportRef = useRef<WebSessionTransport | undefined>(undefined);
  const connectionStateRef = useRef<ConnectionState>(initialConnectionState());
  const everOpenedRef = useRef(false);
  const detailRef = useRef(detail);
  const lastActivityAtRef = useRef(Date.now());
  const onConfigErrorRef = useRef(onConfigError);
  onConfigErrorRef.current = onConfigError;

  const connected = connectionStateAllowsSend(connectionState);

  useEffect(() => {
    setModelState({ confirmedModel: DEFAULT_SESSION_MODEL });
  }, [handle]);

  const applyHostSessionTitle = useCallback((title: string | undefined) => {
    setModelState((prev) => ({ ...prev, sessionTitle: title }));
  }, []);

  detailRef.current = detail;
  connectionStateRef.current = connectionState;

  const markActivity = useCallback(() => {
    lastActivityAtRef.current = Date.now();
    setActivityAgeMs(0);
  }, []);

  const invalidateSession = useCallback(() => {
    if (handle) clearSessionTransportState(handle);
  }, [handle]);

  const applyHostSessionModel = useCallback((nextModel: string) => {
    const next = nextModel.trim() || DEFAULT_SESSION_MODEL;
    setModelState((prev) => ({ ...prev, confirmedModel: next }));
    writeSessionModel(next);
  }, []);

  const applyHostConfigOptions = useCallback((options: LiveSessionConfigOption[] | undefined) => {
    setModelState((prev) => {
      const next = modelFromOptions(options);
      const confirmedModel = next ?? prev.confirmedModel;
      if (next) writeSessionModel(next);
      return { ...prev, confirmedModel, configOptions: options };
    });
  }, []);

  const applyHostAvailableCommands = useCallback(
    (commands: LiveAvailableCommand[] | undefined) => {
      setModelState((prev) => ({ ...prev, availableCommands: commands }));
    },
    [],
  );

  const applyHostPromptCapabilities = useCallback(
    (capabilities: LivePromptCapabilities | undefined) => {
      setModelState((prev) => ({ ...prev, promptCapabilities: capabilities }));
    },
    [],
  );

  useSessionConnection({
    handle,
    dispatch: dispatch as (action: ChatSessionAction) => void,
    detailRef,
    transportRef,
    connectionStateRef,
    everOpenedRef,
    onActivity: markActivity,
    setConnectionState,
    setEverOpened,
    onSessionInvalidated: invalidateSession,
    onSessionModel: applyHostSessionModel,
    onSessionConfigOptions: applyHostConfigOptions,
    onSessionAvailableCommands: applyHostAvailableCommands,
    onSessionPromptCapabilities: applyHostPromptCapabilities,
    onSessionTitle: applyHostSessionTitle,
    onSessionModelRejected: () => {},
    onConfigError: (message) => onConfigErrorRef.current?.(message),
  });

  useEffect(() => {
    if (!view.turn.busy) return;
    const timer = window.setInterval(
      () => setActivityAgeMs(Date.now() - lastActivityAtRef.current),
      30_000,
    );
    return () => window.clearInterval(timer);
  }, [view.turn.busy]);

  const sendPrompt = useCallback(
    (text: string, contentBlocks: PromptContentBlockWire[] = []): boolean => {
      const trimmed = text.trim();
      if (
        !trimmed ||
        !connected ||
        view.context.state === "unavailable" ||
        view.context.transcriptError !== undefined
      ) {
        return false;
      }
      if (!transportRef.current?.sendPrompt(trimmed, contentBlocks)) return false;
      if (!view.turn.busy) markActivity();
      dispatch({ type: "prompt", text: trimmed });
      return true;
    },
    [connected, markActivity, view.context.state, view.context.transcriptError, view.turn.busy],
  );

  const sendCancel = useCallback(() => {
    transportRef.current?.sendCancel();
  }, []);

  const markStopped = useCallback(() => {
    dispatch({ type: "event", event: { type: "system_message", text: "Stopped" } });
  }, []);

  const applyConfigOption = useCallback((configId: string, value: string | boolean) => {
    transportRef.current?.setConfigOption(configId, value);
  }, []);

  const applyModel = useCallback((catalogId: string) => {
    transportRef.current?.setModel(catalogId);
  }, []);

  const retryRestore = useCallback(() => {
    transportRef.current?.retryRestore();
  }, []);

  const startNewContext = useCallback(() => {
    transportRef.current?.startNewContext();
  }, []);

  const respondPermission = useCallback(
    (approved: boolean) => {
      const decision = view.permission.decision;
      if (!decision || !connected) return;
      transportRef.current?.respondPermission(decision.requestId, approved);
      dispatch({ type: "decided" });
    },
    [connected, view.permission.decision],
  );

  const respondElicitation = useCallback(
    (
      action: "accept" | "decline" | "cancel",
      content?: Record<string, string | number | boolean | string[]>,
    ) => {
      const decision = view.elicitation.decision;
      if (!decision || !connected) return;
      transportRef.current?.respondElicitation(decision.requestId, action, content);
      dispatch({ type: "elicitation_answered" });
    },
    [connected, view.elicitation.decision],
  );

  const handleMutated = useCallback(() => {
    if (handle) clearSessionTransportState(handle);
    onMutated?.();
  }, [handle, onMutated]);

  return {
    view,
    connected,
    connectionState,
    everOpened,
    activityAgeMs,
    sessionModel: modelState.confirmedModel,
    sessionConfigOptions: modelState.configOptions,
    sessionAvailableCommands: modelState.availableCommands,
    sessionPromptCapabilities: modelState.promptCapabilities,
    sessionTitle: modelState.sessionTitle,
    transportRef: transportRef as MutableRefObject<WebSessionTransport | undefined>,
    sendPrompt,
    sendCancel,
    markStopped,
    applyConfigOption,
    applyModel,
    retryRestore,
    startNewContext,
    respondPermission,
    respondElicitation,
    onMutated: handleMutated,
  };
}
