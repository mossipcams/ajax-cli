import { useEffect, type MutableRefObject, type RefObject } from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import {
  connectWebSessionTransport,
  type SessionSnapshot,
  type WebSessionTransport,
} from "@/shared/lib/webSessionTransport";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import { MessageBuffer } from "./messageBuffer";
import {
  explainOpenFailure,
  OPEN_FAILURE,
  type SessionAction,
} from "./sessionThread";
import {
  MAX_HANDSHAKE_ATTEMPTS,
  RECONNECT_BASE_MS,
  RECONNECT_MAX_MS,
} from "./sessionChatSeed";
import { isSessionModelChangeFailure } from "./sessionModel";

type Dispatch = (action: SessionAction) => void;

interface Options {
  handle: string | null;
  dispatch: Dispatch;
  detailRef: RefObject<BrowserTaskDetail | null>;
  transportRef: MutableRefObject<WebSessionTransport | undefined>;
  connectedRef: MutableRefObject<boolean>;
  everOpenedRef: MutableRefObject<boolean>;
  onActivity: () => void;
  setConnected: (connected: boolean) => void;
  setEverOpened: (everOpened: boolean) => void;
  onSessionInvalidated?: () => void;
  /** Host snapshot model for the live session (task metadata, not localStorage). */
  onSessionModel?: (model: string) => void;
  /** Live advertised ACP config options from the host snapshot. */
  onSessionConfigOptions?: (options: LiveSessionConfigOption[] | undefined) => void;
  /** Revert an optimistic in-session model change after a host error. */
  onSessionModelRejected?: () => void;
}

/** Connect/reconnect contract: host owns the prompt queue; the browser does not recreate it. */
export function useSessionTransport({
  handle,
  dispatch,
  detailRef,
  transportRef,
  connectedRef,
  everOpenedRef,
  onActivity,
  setConnected,
  setEverOpened,
  onSessionInvalidated,
  onSessionModel,
  onSessionConfigOptions,
  onSessionModelRejected,
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

    const scheduleReconnect = () => {
      if (disposed || !reconnecting) return;
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
      onSessionConfigOptions?.(snapshot.sessionConfigOptions);
    };

    const open = () => {
      transportRef.current?.dispose();
      transportRef.current = undefined;
      buffer?.dispose();
      buffer = new MessageBuffer((event) => dispatch({ type: "event", event }));
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
            connectedRef.current = true;
            setConnected(true);
          },
          onEvent: (event) => {
            onActivity();
            if (event.type === "error" && isSessionModelChangeFailure(event.message)) {
              onSessionModelRejected?.();
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
            connectedRef.current = false;
            setConnected(false);
            if (disposed) return;
            reconnecting = true;
            if (!everOpenedRef.current) {
              handshakeAttempts += 1;
              if (handshakeAttempts > MAX_HANDSHAKE_ATTEMPTS) {
                reconnecting = false;
                onSessionInvalidated?.();
                dispatch({
                  type: "event",
                  event: {
                    type: "error",
                    message: "Lost the session connection. Reopen the task to try again.",
                  },
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
      connectedRef.current = false;
      setConnected(false);
    };
  }, [handle]);
}
