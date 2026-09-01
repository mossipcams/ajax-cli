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
  /** Host snapshot model for the live session (task metadata, not localStorage). */
  onSessionModel?: (model: string) => void;
  /** Live advertised ACP config options from the host snapshot. */
  onSessionConfigOptions?: (options: LiveSessionConfigOption[] | undefined) => void;
  /** Live advertised ACP slash commands from the host snapshot. */
  onSessionAvailableCommands?: (commands: LiveAvailableCommand[] | undefined) => void;
  /** Live advertised ACP prompt capabilities from the host snapshot. */
  onSessionPromptCapabilities?: (capabilities: LivePromptCapabilities | undefined) => void;
  /** Agent-reported session title from the host snapshot. */
  onSessionTitle?: (title: string | undefined) => void;
  /** Revert an optimistic in-session model change after a host error. */
  onSessionModelRejected?: () => void;
  /** Surface config-option apply failures as dismissable notices. */
  onConfigError?: (message: string) => void;
}

/** Connect/reconnect contract: host owns the prompt queue; the browser does not recreate it. */
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
}: Options): void {
  useEffect(() => {
    if (!handle) return;
    let disposed = false;
    let handshakeAttempts = 0;
    let reconnectAttempts = 0;
    let reconnecting = false;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    // Coalesces streamed ACP text with requestAnimationFrame so the reducer sees
    // full-content updates during the turn without one dispatch per token.
    let buffer: MessageBuffer | undefined;
    /** Survives in-page reconnect; cleared on full reload with the reducer. */
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
            // The `ready` event already flushes via onEvent; this is a
            // belt-and-suspenders flush for any replayed text that arrived
            // before the handshake completed, so history renders promptly.
            buffer?.flushAll();
            handshakeAttempts = 0;
            reconnectAttempts = 0;
            everOpenedRef.current = true;
            setEverOpened(true);
            reconnecting = false;
            onSessionModel?.(nextModel);
            setState("connected");
          },
          onEvent: (event) => {
            onActivity();
            if (event.type === "error" && isSessionModelChangeFailure(event.message)) {
              onSessionModelRejected?.();
            }
            if (event.type === "error" && isSessionConfigChangeFailure(event.message)) {
              onConfigError?.(event.message);
            }
            // The socket cannot report why an upgrade was refused, so swap its
            // blank failure for the reason the task detail already carries.
            if (event.type === "error" && event.message === OPEN_FAILURE) {
              buffer?.push({
                type: "error",
                message: explainOpenFailure(detailRef.current),
              });
              return;
            }
            // Streamed text is rAF-coalesced to the latest full content per
            // itemId; boundary events flush any pending lane first so ordering
            // is preserved.
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
