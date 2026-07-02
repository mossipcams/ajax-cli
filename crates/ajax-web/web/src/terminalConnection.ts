/**
 * WebSocket lifecycle for the raw task terminal bridge.
 *
 * Mobile Safari drops the socket whenever it backgrounds the tab, and a failed
 * tmux attach closes it too. A dead socket must recover on its own (capped
 * exponential backoff) and immediately on foreground, rather than stranding
 * the user on a frozen pane. This module owns that lifecycle plus frame
 * decoding; the component only reacts through the event callbacks.
 */

import { openTaskTerminalSocket } from "./api";

export type TerminalConnectionStatus = "connecting" | "connected" | "reconnecting";

export interface TerminalConnectionEvents {
  /** Decoded PTY output, ready for term.write(). */
  onOutput(text: string): void;
  /** A structured `error` frame from the bridge (e.g. failed tmux attach). */
  onServerError(message: string): void;
  onStatus(status: TerminalConnectionStatus): void;
  /** Every successful open; `isReconnect` when a previous socket died first. */
  onOpen(isReconnect: boolean): void;
}

export interface TerminalConnection {
  isOpen(): boolean;
  sendInput(data: string): void;
  sendResize(cols: number, rows: number): void;
  /** Manual reconnect: skip the backoff and dial immediately. */
  reconnectNow(): void;
  dispose(): void;
}

const RECONNECT_MAX_DELAY_MS = 15000;

export function connectTaskTerminal(
  handle: string,
  events: TerminalConnectionEvents,
): TerminalConnection {
  let socket: WebSocket;
  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  let status: TerminalConnectionStatus = "connecting";
  // Streaming decoder: a multi-byte UTF-8 sequence may split across frames.
  const outputDecoder = new TextDecoder();

  const setStatus = (next: TerminalConnectionStatus) => {
    status = next;
    events.onStatus(next);
  };

  const readMessageData = async (data: unknown): Promise<string> => {
    if (typeof data === "string") return data;
    if (data instanceof Blob) {
      // Every supported browser has Blob.text(), but jsdom's Blob does not —
      // the FileReader fallback is what the component tests exercise.
      if ("text" in data && typeof data.text === "function") return data.text();
      return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.addEventListener("load", () => resolve(String(reader.result ?? "")));
        reader.addEventListener("error", () => reject(reader.error));
        reader.readAsText(data);
      });
    }
    if (data instanceof ArrayBuffer) return new TextDecoder().decode(data);
    return String(data);
  };

  const onSocketMessage = async (event: MessageEvent) => {
    const data = await readMessageData(event.data);
    try {
      const payload = JSON.parse(data) as { type?: string; data?: string };
      if (payload.type === "output" && payload.data) {
        const binary = atob(payload.data);
        const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
        events.onOutput(outputDecoder.decode(bytes, { stream: true }));
      } else if (payload.type === "error" && "error" in payload && payload.error) {
        events.onServerError(String(payload.error));
      }
    } catch {
      events.onOutput(data);
    }
  };

  const scheduleReconnect = () => {
    if (disposed) return;
    setStatus("reconnecting");
    const delay = Math.min(RECONNECT_MAX_DELAY_MS, 1000 * 2 ** reconnectAttempts);
    reconnectAttempts += 1;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(() => {
      if (!disposed) connect();
    }, delay);
  };

  const reconnectNow = () => {
    if (disposed) return;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = undefined;
    }
    reconnectAttempts = 0;
    setStatus("connecting");
    connect();
  };

  function connect() {
    socket = openTaskTerminalSocket(handle);
    socket.addEventListener("open", () => {
      // A successful open resets the backoff. A fresh tmux attach repaints the
      // pane and the resize-on-open makes tmux redraw at the real size, so no
      // explicit refresh frame is needed on reconnect.
      const isReconnect = reconnectAttempts > 0;
      reconnectAttempts = 0;
      setStatus("connected");
      events.onOpen(isReconnect);
    });
    socket.addEventListener("message", onSocketMessage);
    // An error is followed by close; let the close handler own reconnect so we
    // never schedule it twice.
    socket.addEventListener("error", () => {});
    socket.addEventListener("close", () => {
      if (disposed) return;
      scheduleReconnect();
    });
  }

  // Reconnect immediately when the tab returns to the foreground instead of
  // waiting out the backoff (mobile Safari kills the socket on background).
  const onVisibility = () => {
    if (document.visibilityState === "visible" && status === "reconnecting") {
      reconnectNow();
    }
  };
  document.addEventListener("visibilitychange", onVisibility);

  connect();

  return {
    isOpen: () => socket.readyState === WebSocket.OPEN,
    sendInput(data: string) {
      if (socket.readyState !== WebSocket.OPEN) return;
      socket.send(JSON.stringify({ type: "input", data }));
    },
    sendResize(cols: number, rows: number) {
      if (socket.readyState !== WebSocket.OPEN) return;
      socket.send(JSON.stringify({ type: "resize", cols, rows }));
    },
    reconnectNow,
    dispose() {
      disposed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      document.removeEventListener("visibilitychange", onVisibility);
      socket.close();
    },
  };
}
