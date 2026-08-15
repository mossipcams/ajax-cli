// Authenticated orchestration-chat WebSocket transport (ACP-primary; not PTY).

const OPEN_READY_STATE = 1;

/** Match host FIFO cap (`web_session::MAX_QUEUED_PROMPTS`). */
const MAX_QUEUED_PROMPTS = 8;

/** Emitted when the upgrade is refused. The browser cannot expose the HTTP
 * status or body of a failed WebSocket handshake, so this string carries no
 * reason — callers recover one from task truth. */
export const OPEN_FAILURE = "Session WebSocket failed to open";

type SocketListener = (event: Event | MessageEvent) => void;

export interface WebSessionSocket {
  readyState: number;
  send(data: string): void;
  close(): void;
  addEventListener(type: string, listener: SocketListener): void;
  removeEventListener(type: string, listener: SocketListener): void;
}

export interface WebSessionTransportPlatform {
  openSocket(url: string): WebSessionSocket;
}

export type WebSessionServerEvent =
  | { type: "ready"; model?: string; busy?: boolean }
  | { type: "message"; role: string; text: string }
  | { type: "artifact"; kind: string; title?: string | null; body?: string | null }
  | {
      type: "tool_call";
      callId: string;
      title: string;
      kind: string;
      status: string;
      locations?: string[];
    }
  | { type: "plan"; entries: { content: string; status: string }[] }
  | {
      type: "permission_request";
      requestId: string;
      title?: string | null;
      detail?: string | null;
    }
  | { type: "permission_resolved"; requestId: string; approved: boolean }
  | { type: "status"; state: string; detail?: string | null }
  | { type: "turn_end"; stopReason?: string | null }
  | { type: "error"; message: string };

export interface WebSessionTransportCallbacks {
  onReady: (model: string) => void;
  onEvent: (event: WebSessionServerEvent) => void;
  onClosed: () => void;
}

export interface WebSessionTransport {
  sendPrompt(text: string): void;
  sendCancel(keepQueue?: boolean): void;
  setModel(model: string): void;
  respondPermission(requestId: string, approved: boolean, reason?: string): void;
  dispose(): void;
}

function sessionSocketUrl(handle: string, model?: string): string {
  const protocol =
    typeof location !== "undefined" && location.protocol === "https:" ? "wss:" : "ws:";
  const host = typeof location !== "undefined" ? location.host : "localhost";
  const base = `${protocol}//${host}/api/tasks/${encodeURIComponent(handle)}/session`;
  if (!model) return base;
  return `${base}?model=${encodeURIComponent(model)}`;
}

function wrapNativeSocket(socket: WebSocket): WebSessionSocket {
  return {
    get readyState() {
      return socket.readyState;
    },
    send(data) {
      socket.send(data);
    },
    close() {
      socket.close();
    },
    addEventListener(type, listener) {
      socket.addEventListener?.(type, listener as EventListener);
    },
    removeEventListener(type, listener) {
      socket.removeEventListener?.(type, listener as EventListener);
    },
  };
}

export function createBrowserWebSessionPlatform(): WebSessionTransportPlatform {
  return {
    openSocket(url) {
      return wrapNativeSocket(new WebSocket(url));
    },
  };
}

function parseServerEvent(raw: string): WebSessionServerEvent | null {
  try {
    const payload = JSON.parse(raw) as WebSessionServerEvent;
    if (!payload || typeof payload !== "object" || !("type" in payload)) {
      return null;
    }
    return payload;
  } catch {
    return null;
  }
}

function waitForSocketOpen(target: WebSessionSocket): Promise<void> {
  if (target.readyState === OPEN_READY_STATE) {
    return Promise.resolve();
  }
  return new Promise((resolve, reject) => {
    const onOpen = () => {
      cleanup();
      resolve();
    };
    const onError = () => {
      cleanup();
      reject(new Error("Session WebSocket failed to open"));
    };
    const onClose = () => {
      cleanup();
      reject(new Error("Session WebSocket closed before open"));
    };
    const cleanup = () => {
      target.removeEventListener("open", onOpen);
      target.removeEventListener("error", onError);
      target.removeEventListener("close", onClose);
    };
    target.addEventListener("open", onOpen);
    target.addEventListener("error", onError);
    target.addEventListener("close", onClose);
  });
}

export function connectWebSessionTransport(
  handle: string,
  callbacks: WebSessionTransportCallbacks,
  platform: WebSessionTransportPlatform = createBrowserWebSessionPlatform(),
  model = "auto",
): WebSessionTransport {
  let socket: WebSessionSocket | undefined;
  let ready = false;
  let disposed = false;
  const pendingPrompts: string[] = [];

  const messageListener: SocketListener = (event) => {
    const messageEvent = event as MessageEvent;
    if (typeof messageEvent.data !== "string") return;
    const parsed = parseServerEvent(messageEvent.data);
    if (!parsed) return;
    if (parsed.type === "ready") {
      ready = true;
      const nextModel =
        typeof parsed.model === "string" && parsed.model.trim() ? parsed.model.trim() : model;
      callbacks.onReady(nextModel);
      while (pendingPrompts.length > 0) {
        const text = pendingPrompts.shift();
        if (text) sendPromptNow(text);
      }
      return;
    }
    callbacks.onEvent(parsed);
  };

  const closeListener: SocketListener = () => {
    if (!disposed) callbacks.onClosed();
  };

  function sendJson(payload: Record<string, unknown>) {
    if (!socket || socket.readyState !== OPEN_READY_STATE) return;
    socket.send(JSON.stringify(payload));
  }

  function sendPromptNow(text: string) {
    sendJson({ type: "prompt", text });
  }

  socket = platform.openSocket(sessionSocketUrl(handle, model));
  socket.addEventListener("message", messageListener);
  // An error is followed by close; let the close handler own onClosed so we
  // never schedule reconnect twice.
  socket.addEventListener("close", closeListener);
  void waitForSocketOpen(socket).catch(() => {
    pendingPrompts.length = 0;
    callbacks.onEvent({ type: "error", message: OPEN_FAILURE });
  });

  return {
    sendPrompt(text) {
      const trimmed = text.trim();
      if (!trimmed) return;
      if (ready) {
        sendPromptNow(trimmed);
        return;
      }
      if (pendingPrompts.length >= MAX_QUEUED_PROMPTS) {
        pendingPrompts.shift();
      }
      pendingPrompts.push(trimmed);
    },
    sendCancel(keepQueue = false) {
      if (!keepQueue) {
        pendingPrompts.length = 0;
      }
      sendJson(keepQueue ? { type: "cancel", keepQueue: true } : { type: "cancel" });
    },
    setModel(nextModel) {
      const trimmed = nextModel.trim() || "auto";
      sendJson({ type: "set_model", model: trimmed });
    },
    respondPermission(requestId, approved, reason) {
      sendJson({
        type: "permission",
        requestId,
        approved,
        ...(reason ? { reason } : {}),
      });
    },
    dispose() {
      disposed = true;
      socket?.removeEventListener("message", messageListener);
      socket?.removeEventListener("close", closeListener);
      try {
        socket?.close();
      } catch {
        // ignore close races
      }
      socket = undefined;
    },
  };
}
