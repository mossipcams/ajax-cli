import { WEB_SESSION_PROTOCOL_VERSION } from "./types";
import type {
  SessionAttentionItem,
  SessionAttentionKind,
  SessionAttentionResponse,
  WebSessionSymbolContext,
} from "./types";

const OPEN_READY_STATE = 1;

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
  onConnectionStatus(status: "connecting" | "connected" | "error" | "closed"): void;
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

  const detachListeners = () => {
    if (!socket) return;
    if (messageListener) socket.removeEventListener("message", messageListener);
    if (errorListener) socket.removeEventListener("error", errorListener);
    if (closeListener) socket.removeEventListener("close", closeListener);
    messageListener = undefined;
    errorListener = undefined;
    closeListener = undefined;
  };

  const fail = (message: string) => {
    if (disposed) return;
    callbacks.onConnectionStatus("error");
    callbacks.onError(message);
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
          typeof payload.message === "string"
            ? payload.message
            : typeof payload.code === "string"
              ? payload.code
              : "Session error",
        );
        break;
      case "session.closed":
        callbacks.onConnectionStatus("closed");
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
            typeof payload.message === "string" ? payload.message : "Attention reply failed",
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

  callbacks.onConnectionStatus("connecting");
  socket = platform.openSocket(webSessionSocketUrl(handle));
  messageListener = handleServerMessage;
  errorListener = () => fail("Web session connection failed");
  closeListener = () => {
    detachListeners();
    if (disposed) return;
    callbacks.onConnectionStatus("closed");
    callbacks.onClosed();
  };
  socket.addEventListener("message", messageListener);
  socket.addEventListener("error", errorListener);
  socket.addEventListener("open", () => {
    if (disposed) return;
    callbacks.onConnectionStatus("connected");
  });
  socket.addEventListener("close", closeListener);

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
    dispose() {
      disposed = true;
      detachListeners();
      try {
        socket?.close();
      } catch {
        // ignore close races
      }
      socket = undefined;
    },
  };
}
