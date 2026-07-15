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

export type TerminalConnectionStatus =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "unavailable";

export interface TerminalConnectionEvents {
  /** Decoded PTY output, ready for term.write(). */
  onOutput(text: string): void;
  /** A structured `error` frame from the bridge (e.g. failed tmux attach). */
  onServerError(message: string): void;
  onStatus(status: TerminalConnectionStatus): void;
  /** Every successful open; `isReconnect` when a previous socket had opened.
   * `seeded` = this dial asked the bridge for the history seed; unseeded reconnects must keep the local buffer. */
  onOpen(isReconnect: boolean, seeded: boolean): void;
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
const IMMEDIATE_FAILURE_LIMIT = 5;

export function connectTaskTerminal(
  handle: string,
  events: TerminalConnectionEvents,
): TerminalConnection {
  let socket: WebSocket;
  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  let everOpened = false;
  let attachFailed = false;
  let disposed = false;
  let status: TerminalConnectionStatus = "connecting";
  let lastDialSeeded = true;
  // Streaming decoder: a multi-byte UTF-8 sequence may split across frames.
  const outputDecoder = new TextDecoder();
  const inputEncoder = new TextEncoder();

  const setStatus = (next: TerminalConnectionStatus) => {
    status = next;
    events.onStatus(next);
  };

  const readBlobArrayBuffer = async (blob: Blob): Promise<ArrayBuffer> => {
    if ("arrayBuffer" in blob && typeof blob.arrayBuffer === "function") {
      return blob.arrayBuffer();
    }
    // jsdom Blob may lack arrayBuffer(); FileReader is what component tests use.
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.addEventListener("load", () => resolve(reader.result as ArrayBuffer));
      reader.addEventListener("error", () => reject(reader.error));
      reader.readAsArrayBuffer(blob);
    });
  };

  const bytesFromBinaryData = async (data: unknown): Promise<Uint8Array | null> => {
    if (data instanceof ArrayBuffer) return new Uint8Array(data);
    if (ArrayBuffer.isView(data)) {
      return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
    }
    // Cross-realm ArrayBuffer (jsdom MessageEvent) may fail instanceof.
    if (
      data != null &&
      typeof data === "object" &&
      Object.prototype.toString.call(data) === "[object ArrayBuffer]"
    ) {
      return new Uint8Array(data as ArrayBuffer);
    }
    if (data instanceof Blob) {
      return new Uint8Array(await readBlobArrayBuffer(data));
    }
    return null;
  };

  const handleJsonControlFrame = (text: string): boolean => {
    let payload: { type?: string; data?: string; error?: unknown };
    try {
      payload = JSON.parse(text) as { type?: string; data?: string; error?: unknown };
    } catch {
      return false;
    }
    if (payload.type === "output" && payload.data) {
      // Legacy JSON+base64 output (one-release compat).
      const binary = atob(payload.data);
      const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
      events.onOutput(outputDecoder.decode(bytes, { stream: true }));
      return true;
    }
    if (payload.type === "error" && payload.error) {
      attachFailed = true;
      events.onServerError(String(payload.error));
      return true;
    }
    return false;
  };

  const onSocketMessage = async (event: MessageEvent) => {
    const raw = event.data;

    // Binary PTY output (server Message::Binary). Default browser binaryType is Blob.
    const binaryBytes = await bytesFromBinaryData(raw);
    if (binaryBytes) {
      // JSON control/error (or legacy output) may arrive as a Blob of UTF-8 JSON.
      if (binaryBytes.length > 0 && binaryBytes[0] === 0x7b /* { */) {
        const asText = new TextDecoder().decode(binaryBytes);
        if (handleJsonControlFrame(asText)) return;
      }
      events.onOutput(outputDecoder.decode(binaryBytes, { stream: true }));
      return;
    }

    if (typeof raw !== "string") {
      events.onOutput(String(raw));
      return;
    }

    // Text frames: JSON control/error/legacy output, else raw pass-through.
    if (!handleJsonControlFrame(raw)) {
      events.onOutput(raw);
    }
  };

  const scheduleReconnect = () => {
    if (disposed) return;
    setStatus("reconnecting");
    const delay = Math.min(RECONNECT_MAX_DELAY_MS, 1000 * 2 ** reconnectAttempts);
    reconnectAttempts += 1;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(() => {
      if (!disposed) connect(false);
    }, delay);
  };

  const redialNow = (seedHistory: boolean) => {
    if (disposed) return;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = undefined;
    }
    reconnectAttempts = 0;
    attachFailed = false;
    setStatus("connecting");
    connect(seedHistory);
  };

  const reconnectNow = () => redialNow(true);

  function connect(seedHistory: boolean) {
    lastDialSeeded = seedHistory;
    socket = openTaskTerminalSocket(handle, seedHistory);
    socket.addEventListener("open", () => {
      // A successful open resets the backoff. A fresh tmux attach repaints the
      // pane and the resize-on-open makes tmux redraw at the real size, so no
      // explicit refresh frame is needed on reconnect.
      const isReconnect = everOpened;
      everOpened = true;
      reconnectAttempts = 0;
      attachFailed = false;
      setStatus("connected");
      events.onOpen(isReconnect, lastDialSeeded);
    });
    socket.addEventListener("message", onSocketMessage);
    // An error is followed by close; let the close handler own reconnect so we
    // never schedule it twice.
    socket.addEventListener("error", () => {});
    socket.addEventListener("close", () => {
      if (disposed) return;
      if (attachFailed || (!everOpened && reconnectAttempts >= IMMEDIATE_FAILURE_LIMIT)) {
        setStatus("unavailable");
        return;
      }
      scheduleReconnect();
    });
  }

  // Reconnect immediately when the tab returns to the foreground instead of
  // waiting out the backoff (mobile Safari kills the socket on background).
  const onVisibility = () => {
    if (document.visibilityState === "visible" && status === "reconnecting") {
      redialNow(false);
    }
  };
  document.addEventListener("visibilitychange", onVisibility);

  connect(true);

  return {
    isOpen: () => socket.readyState === WebSocket.OPEN,
    sendInput(data: string) {
      if (socket.readyState !== WebSocket.OPEN) return;
      const MAX_INPUT_FRAME_BYTES = 4096;
      const bytes = inputEncoder.encode(data);
      for (let offset = 0; offset < bytes.byteLength; offset += MAX_INPUT_FRAME_BYTES) {
        const end = Math.min(offset + MAX_INPUT_FRAME_BYTES, bytes.byteLength);
        socket.send(bytes.subarray(offset, end));
      }
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
