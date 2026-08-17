import { useEffect, type MutableRefObject, type RefObject } from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import {
  connectWebSessionTransport,
  type WebSessionTransport,
} from "@/shared/lib/webSessionTransport";
import { MessageBuffer } from "./messageBuffer";
import {
  explainOpenFailure,
  OPEN_FAILURE,
  type SessionAction,
} from "./sessionThread";
import { readSessionModel, writeSessionModel } from "./sessionModel";
import {
  MAX_HANDSHAKE_ATTEMPTS,
  RECONNECT_BASE_MS,
  RECONNECT_MAX_MS,
} from "./sessionChatSeed";

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
}: Options): void {
  useEffect(() => {
    if (!handle) return;
    let disposed = false;
    let handshakeAttempts = 0;
    let reconnectAttempts = 0;
    let reconnecting = false;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    // Coalesces ACP message chunks so the assistant response renders as
    // paragraphs, not a token stream (issue #904, typewriter-free). Recreated
    // per connection so a reset clears any text buffered from the previous
    // connection.
    let buffer: MessageBuffer | undefined;
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

    const open = () => {
      transportRef.current?.dispose();
      transportRef.current = undefined;
      buffer?.dispose();
      buffer = new MessageBuffer((event) => dispatch({ type: "event", event }));
      // Clear before the host replays its durable transcript, never after.
      dispatch({ type: "reset" });
      const transport = connectWebSessionTransport(
        handle,
        {
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
            writeSessionModel(nextModel);
            connectedRef.current = true;
            setConnected(true);
          },
          onEvent: (event) => {
            onActivity();
            // The socket cannot report why an upgrade was refused, so swap its
            // blank failure for the reason the task detail already carries.
            if (event.type === "error" && event.message === OPEN_FAILURE) {
              buffer?.push({
                type: "error",
                message: explainOpenFailure(detailRef.current),
              });
              return;
            }
            // Streamed text is held for the turn and flushed as paragraphs at a
            // boundary (turn_end, a tool call, ready); everything else flushes
            // pending text first so ordering is preserved.
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
        readSessionModel(),
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
