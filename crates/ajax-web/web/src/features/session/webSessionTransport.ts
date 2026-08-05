import { WEB_SESSION_PROTOCOL_VERSION } from "./types";
import type {
  SessionAttentionItem,
  SessionAttentionKind,
  SessionAttentionResponse,
  WebSessionConnectionStatus,
  WebSessionSymbolContext,
} from "./types";

const OPEN_READY_STATE = 1;
const RECONNECT_MAX_DELAY_MS = 15000;
const IMMEDIATE_FAILURE_LIMIT = 5;
const STABLE_OPEN_MS = 1000;

export interface WebSessionSocket {
  readyState: number;
  send(data: string): void;
  close(): void;
  addEventListener(type: string, listener: EventListener): void;
  removeEventListener(type: string, listener: EventListener): void;
}

export interface WebSessionTransportPlatform {
  openSocket(url: string): WebSessionSocket;
}

export interface WebSessionTransportCallbacks {
  onConnectionStatus(status: WebSessionConnectionStatus): void;
  onSessionReady(sessionId: string): void;
  onRunStatus(status: "running" | "waiting"): void;
  onAssistantDelta(text: string): void;
  onSettled(): void;
  onError(message: string): void;
  onClosed(): void;
  onAttentionRequired?(item: SessionAttentionItem): void;
  onAttentionCleared?(handle: string, requestId: string): void;
  onAttentionError?(handle: string, requestId: string, message: string): void;
}

export interface WebSessionTransport {
  sendPrompt(message: string): void;
  sendAbort(): void;
  respondAttention(
    targetHandle: string,
    requestId: string,
    response: SessionAttentionResponse,
  ): void;
  /** Manual reconnect: skip backoff and dial immediately. */
  reconnectNow(): void;
  dispose(): void;
}

export function composeWebSessionPrompt(
  userMessage: string,
  symbols: WebSessionSymbolContext[],
): string {
  const question = userMessage.trim();
  if (symbols.length === 0) {
    return question;
  }

  const contextBlocks = symbols.map((symbol) => {
    const header = `### ${symbol.path} — ${symbol.kind} \`${symbol.name}\` (lines ${symbol.startLine}-${symbol.endLine})`;
    return `${header}\n\`\`\`\n${symbol.source.trim()}\n\`\`\``;
  });

  return `## Attached context\n\n${contextBlocks.join("\n\n")}\n\n## Question\n\n${question}`;
}

export function webSessionSocketUrl(handle: string): string {
  const protocol =
    typeof location !== "undefined" && location.protocol === "https:" ? "wss:" : "ws:";
  const host = typeof location !== "undefined" ? location.host : "localhost";
  return `${protocol}//${host}/api/tasks/${encodeURIComponent(handle)}/web-session`;
}

export function createBrowserWebSessionPlatform(): WebSessionTransportPlatform {
  return {
    openSocket(url) {
      return new WebSocket(url) as unknown as WebSessionSocket;
    },
  };
}

function parseAttentionKind(value: unknown): SessionAttentionKind | null {
  if (
    value === "permission" ||
    value === "question" ||
    value === "failed" ||
    value === "review"
  ) {
    return value;
  }
  return null;
}

export function clarifyWebSessionError(message: string): string {
  const lower = message.toLowerCase();
  if (lower.includes("cursor_login") || lower.includes("authenticate")) {
    return "Cursor login required on the host. Sign in with `agent` / Cursor, then Retry.";
  }
  if (lower.includes("not found") || lower.includes("no such file") || lower.includes("spawn")) {
    return "Cursor agent binary not available on the host. Install or fix PATH, then Retry.";
  }
  if (lower.includes("connection failed") || lower.includes("websocket")) {
    return "Web session connection failed. Check the host and tap Retry.";
  }
  if (lower.includes("attention request is no longer pending") || lower.includes("stale")) {
    return "That attention request is no longer pending.";
  }
  if (lower.includes("hub is gone") || lower.includes("hub gone")) {
    return "Originating session is gone. Open that task or start a new prompt.";
  }
  return message;
}

export function connectWebSession(
  handle: string,
  callbacks: WebSessionTransportCallbacks,
  platform: WebSessionTransportPlatform = createBrowserWebSessionPlatform(),
): WebSessionTransport {
  let socket: WebSessionSocket | undefined;
  let disposed = false;
  let messageListener: EventListener | undefined;
  let errorListener: EventListener | undefined;
  let closeListener: EventListener | undefined;
  let openListener: EventListener | undefined;
  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  let everOpened = false;
  let dialOpened = false;
  let dialOpenedAt: number | undefined;
  let status: WebSessionConnectionStatus = "connecting";
  let visibilityListener: (() => void) | undefined;

  const setStatus = (next: WebSessionConnectionStatus) => {
    status = next;
    callbacks.onConnectionStatus(next);
  };

  const detachListeners = () => {
    if (!socket) return;
    if (messageListener) socket.removeEventListener("message", messageListener);
    if (errorListener) socket.removeEventListener("error", errorListener);
    if (closeListener) socket.removeEventListener("close", closeListener);
    if (openListener) socket.removeEventListener("open", openListener);
    messageListener = undefined;
    errorListener = undefined;
    closeListener = undefined;
    openListener = undefined;
  };

  const clearReconnectTimer = () => {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = undefined;
    }
  };

  const failFatal = (message: string) => {
    if (disposed) return;
    clearReconnectTimer();
    setStatus("error");
    callbacks.onError(clarifyWebSessionError(message));
  };

  const handleServerMessage = (event: Event) => {
    const raw = (event as MessageEvent).data;
    if (typeof raw !== "string") return;
    let payload: Record<string, unknown>;
    try {
      payload = JSON.parse(raw) as Record<string, unknown>;
    } catch {
      return;
    }
    if (payload.version !== WEB_SESSION_PROTOCOL_VERSION) return;
    switch (payload.type) {
      case "session.ready":
        if (typeof payload.sessionId === "string") {
          callbacks.onSessionReady(payload.sessionId);
        }
        break;
      case "session.status":
        if (payload.state === "running" || payload.state === "waiting") {
          callbacks.onRunStatus(payload.state);
        }
        break;
      case "session.assistant_delta":
        if (typeof payload.text === "string") {
          callbacks.onAssistantDelta(payload.text);
        }
        break;
      case "session.settled":
        callbacks.onSettled();
        break;
      case "session.error":
        callbacks.onRunStatus("waiting");
        callbacks.onError(
          clarifyWebSessionError(
            typeof payload.message === "string"
              ? payload.message
              : typeof payload.code === "string"
                ? payload.code
                : "Session error",
          ),
        );
        break;
      case "session.closed":
        setStatus("closed");
        callbacks.onClosed();
        break;
      case "attention.required": {
        const kind = parseAttentionKind(payload.kind);
        if (
          kind &&
          typeof payload.handle === "string" &&
          typeof payload.requestId === "string" &&
          typeof payload.title === "string" &&
          typeof payload.summary === "string"
        ) {
          callbacks.onAttentionRequired?.({
            handle: payload.handle,
            requestId: payload.requestId,
            kind,
            title: payload.title,
            summary: payload.summary,
            options: Array.isArray(payload.options)
              ? payload.options.filter((item): item is string => typeof item === "string")
              : undefined,
          });
        }
        break;
      }
      case "attention.cleared":
        if (typeof payload.handle === "string" && typeof payload.requestId === "string") {
          callbacks.onAttentionCleared?.(payload.handle, payload.requestId);
        }
        break;
      case "attention.error":
        if (typeof payload.handle === "string" && typeof payload.requestId === "string") {
          callbacks.onAttentionError?.(
            payload.handle,
            payload.requestId,
            clarifyWebSessionError(
              typeof payload.message === "string" ? payload.message : "Attention reply failed",
            ),
          );
        }
        break;
      default:
        break;
    }
  };

  const sendControl = (body: Record<string, unknown>) => {
    if (!socket || socket.readyState !== OPEN_READY_STATE) return;
    socket.send(JSON.stringify({ version: WEB_SESSION_PROTOCOL_VERSION, ...body }));
  };

  const dial = () => {
    if (disposed) return;
    clearReconnectTimer();
    detachListeners();
    try {
      socket?.close();
    } catch {
      // ignore
    }
    socket = undefined;
    dialOpened = false;
    dialOpenedAt = undefined;

    if (!everOpened) {
      setStatus("connecting");
    } else {
      setStatus("reconnecting");
    }

    socket = platform.openSocket(webSessionSocketUrl(handle));
    messageListener = handleServerMessage;
    // Error is followed by close; close owns reconnect so we do not double-schedule.
    errorListener = () => {};
    openListener = () => {
      if (disposed) return;
      dialOpened = true;
      dialOpenedAt = Date.now();
      everOpened = true;
      reconnectAttempts = 0;
      setStatus("connected");
    };
    closeListener = () => {
      detachListeners();
      if (disposed) return;
      const stableOpen =
        dialOpened &&
        dialOpenedAt !== undefined &&
        Date.now() - dialOpenedAt >= STABLE_OPEN_MS;
      callbacks.onClosed();
      if (!everOpened && reconnectAttempts >= IMMEDIATE_FAILURE_LIMIT) {
        failFatal("Web session connection failed");
        return;
      }
      if (everOpened && !stableOpen && reconnectAttempts >= IMMEDIATE_FAILURE_LIMIT) {
        failFatal("Web session connection failed");
        return;
      }
      scheduleReconnect(stableOpen);
    };
    socket.addEventListener("message", messageListener);
    socket.addEventListener("error", errorListener);
    socket.addEventListener("open", openListener);
    socket.addEventListener("close", closeListener);
  };

  const scheduleReconnect = (stableOpen = false) => {
    if (disposed) return;
    setStatus("reconnecting");
    const immediate =
      stableOpen &&
      typeof document !== "undefined" &&
      document.visibilityState === "visible" &&
      reconnectAttempts === 0;
    const delay = immediate
      ? 0
      : Math.min(RECONNECT_MAX_DELAY_MS, 1000 * 2 ** reconnectAttempts);
    reconnectAttempts += 1;
    clearReconnectTimer();
    reconnectTimer = setTimeout(() => {
      reconnectTimer = undefined;
      if (disposed) return;
      if (typeof document !== "undefined" && document.visibilityState !== "visible") {
        // Stay reconnecting; visibility handler redials.
        return;
      }
      dial();
    }, delay);
  };

  const reconnectNow = () => {
    if (disposed) return;
    clearReconnectTimer();
    reconnectAttempts = 0;
    dial();
  };

  if (typeof document !== "undefined") {
    visibilityListener = () => {
      if (disposed) return;
      if (document.visibilityState === "visible" && status === "reconnecting") {
        reconnectNow();
      }
    };
    document.addEventListener("visibilitychange", visibilityListener);
  }

  dial();

  return {
    sendPrompt(message: string) {
      const trimmed = message.trim();
      if (!trimmed) return;
      sendControl({ type: "session.prompt", message: trimmed });
    },
    sendAbort() {
      sendControl({ type: "session.abort" });
    },
    respondAttention(targetHandle, requestId, response) {
      sendControl({
        type: "attention.respond",
        targetHandle,
        requestId,
        response,
      });
    },
    reconnectNow,
    dispose() {
      disposed = true;
      clearReconnectTimer();
      detachListeners();
      if (visibilityListener && typeof document !== "undefined") {
        document.removeEventListener("visibilitychange", visibilityListener);
      }
      try {
        socket?.close();
      } catch {
        // ignore close races
      }
      socket = undefined;
    },
  };
}
