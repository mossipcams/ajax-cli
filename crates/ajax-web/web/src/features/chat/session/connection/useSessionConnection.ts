import { useEffect, type MutableRefObject, type RefObject } from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import type { LiveAvailableCommand } from "@/shared/lib/liveSessionCommands";
import type { LivePromptCapabilities } from "@/shared/lib/liveSessionPromptCapabilities";
import {
  connectWebSessionTransport,
  MessageBuffer,
  OPEN_FAILURE,
  type SessionSnapshot,
  type WebSessionServerEvent,
  type WebSessionTransport,
} from "../transport/public";
import { explainOpenFailure } from "../errors";
import type { ChatSessionAction } from "../model";
import { projectWireEvent } from "../projectWireInput";
import {
  MAX_HANDSHAKE_ATTEMPTS,
  RECONNECT_BASE_MS,
  RECONNECT_MAX_MS,
} from "../../sessionChatSeed";
import { isSessionModelChangeFailure, isSessionConfigChangeFailure } from "../../model/public";
import type { ConnectionState } from "./connectionState";

type Dispatch = (action: ChatSessionAction) => void;

function dispatchWireEvent(dispatch: Dispatch, event: WebSessionServerEvent): void {
  const projected = projectWireEvent(event);
  if (projected) dispatch({ type: "event", event: projected });
}

interface Options {
  handle: string | null;
  dispatch: Dispatch;
  detailRef: RefObject<BrowserTaskDetail | null>;
  transportRef: MutableRefObject<WebSessionTransport | undefined>;
  connectionStateRef: MutableRefObject<ConnectionState>;
  everOpenedRef: MutableRefObject<boolean>;
  onActivity: () => void;
  setConnectionState: (state: ConnectionState) => void;
  setEverOpened: (everOpened: boolean) => void;
  onSessionInvalidated?: () => void;
  onSessionModel?: (model: string) => void;
  onSessionConfigOptions?: (options: LiveSessionConfigOption[] | undefined) => void;
  onSessionAvailableCommands?: (commands: LiveAvailableCommand[] | undefined) => void;
  onSessionPromptCapabilities?: (capabilities: LivePromptCapabilities | undefined) => void;
  onSessionTitle?: (title: string | undefined) => void;
  onSessionModelRejected?: () => void;
  onConfigError?: (message: string) => void;
  onRestoreFailure?: () => void;
  onRestoreResolved?: () => void;
}

export function useSessionConnection({
  handle,
  dispatch,
  detailRef,
  transportRef,
  connectionStateRef,
  everOpenedRef,
  onActivity,
  setConnectionState,
  setEverOpened,
  onSessionInvalidated,
  onSessionModel,
  onSessionConfigOptions,
  onSessionAvailableCommands,
  onSessionPromptCapabilities,
  onSessionTitle,
  onSessionModelRejected,
  onConfigError,
  onRestoreFailure,
  onRestoreResolved,
}: Options): void {
  useEffect(() => {
    if (!handle) return;
    let disposed = false;
    let handshakeAttempts = 0;
    let reconnectAttempts = 0;
    let reconnecting = false;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    let buffer: MessageBuffer | undefined;
    let nextToReadCursor: number | undefined;
    everOpenedRef.current = false;
    setEverOpened(false);

    const setState = (state: ConnectionState) => {
      connectionStateRef.current = state;
      setConnectionState(state);
    };

    const scheduleReconnect = () => {
      if (disposed || !reconnecting) return;
      setState("waiting");
      const immediateAfterOpen =
        everOpenedRef.current && document.visibilityState === "visible" && reconnectAttempts === 0;
      const delay = immediateAfterOpen
        ? 0
        : Math.min(RECONNECT_BASE_MS * 2 ** reconnectAttempts, RECONNECT_MAX_MS);
      reconnectAttempts += 1;
      if (retryTimer) clearTimeout(retryTimer);
      retryTimer = setTimeout(() => {
        if (disposed || !reconnecting) return;
        if (document.visibilityState !== "visible") return;
        open();
      }, delay);
    };

    const applySnapshot = (snapshot: SessionSnapshot) => {
      onSessionModel?.(snapshot.model);
      onSessionConfigOptions?.(snapshot.sessionConfigOptions);
      onSessionAvailableCommands?.(snapshot.availableCommands);
      onSessionPromptCapabilities?.(snapshot.promptCapabilities);
      onSessionTitle?.(snapshot.sessionTitle);
    };

    const open = () => {
      setState("connecting");
      transportRef.current?.dispose();
      transportRef.current = undefined;
      buffer?.dispose();
      buffer = new MessageBuffer((event) => dispatchWireEvent(dispatch, event));
      const transport = connectWebSessionTransport(
        handle,
        {
          onCursorAdvance: (cursor) => {
            nextToReadCursor = cursor;
          },
          onSnapshot: applySnapshot,
          onReady: (nextModel) => {
            buffer?.flushAll();
            handshakeAttempts = 0;
            reconnectAttempts = 0;
            everOpenedRef.current = true;
            setEverOpened(true);
            reconnecting = false;
            onSessionModel?.(nextModel);
            onRestoreResolved?.();
            setState("connected");
          },
          onEvent: (event) => {
            onActivity();
            if (event.type === "error" && /ACP restore unavailable/i.test(event.message)) {
              onRestoreFailure?.();
            }
            if (event.type === "error" && isSessionModelChangeFailure(event.message)) {
              onSessionModelRejected?.();
            }
            if (event.type === "error" && isSessionConfigChangeFailure(event.message)) {
              onConfigError?.(event.message);
            }
            if (event.type === "error" && event.message === OPEN_FAILURE) {
              buffer?.push({
                type: "error",
                message: explainOpenFailure(detailRef.current),
              });
              return;
            }
            buffer?.push(event);
          },
          onClosed: () => {
            if (disposed) return;
            reconnecting = true;
            if (!everOpenedRef.current) {
              handshakeAttempts += 1;
              if (handshakeAttempts > MAX_HANDSHAKE_ATTEMPTS) {
                reconnecting = false;
                setState("failed");
                onSessionInvalidated?.();
                dispatchWireEvent(dispatch, {
                  type: "error",
                  message: "Lost the session connection. Reopen the task to try again.",
                });
                return;
              }
            }
            scheduleReconnect();
          },
        },
        undefined,
        undefined,
        nextToReadCursor,
      );
      transportRef.current = transport;
    };

    const onVisibility = () => {
      if (document.visibilityState === "visible" && reconnecting) {
        if (retryTimer) {
          clearTimeout(retryTimer);
          retryTimer = undefined;
        }
        reconnectAttempts = 0;
        open();
      }
    };

    document.addEventListener("visibilitychange", onVisibility);
    open();
    return () => {
      disposed = true;
      reconnecting = false;
      document.removeEventListener("visibilitychange", onVisibility);
      if (retryTimer) clearTimeout(retryTimer);
      buffer?.dispose();
      buffer = undefined;
      transportRef.current?.dispose();
      transportRef.current = undefined;
      setState("disposed");
    };
  }, [handle]);
}
