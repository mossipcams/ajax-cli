import { createTerminalClientId, openTaskTerminalSocket, renewBrowserSession } from "./api";

export type TerminalConnectionStatus =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "unavailable";

export interface TerminalConnectionEvents {
  onOutput(text: string): void;
  onServerError(message: string): void;
  onStatus(status: TerminalConnectionStatus): void;
  onOpen(isReconnect: boolean, seeded: boolean): void;
}

export interface TerminalConnection {
  isOpen(): boolean;
  sendInput(data: string): void;
  sendResize(cols: number, rows: number): void;
  reconnectNow(): void;
  dispose(): void;
}

const RECONNECT_MAX_DELAY_MS = 15000;
const IMMEDIATE_FAILURE_LIMIT = 5;
const STABLE_OPEN_MS = 1000;

export function connectTaskTerminal(
  handle: string,
  events: TerminalConnectionEvents,
): TerminalConnection {
  let socket: WebSocket | undefined;
  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  let everOpened = false;
  let dialOpened = false;
  let dialOpenedAt: number | undefined;
  let unstableOpenFailures = 0;
  let sessionRenewTried = false;
  let attachFailed = false;
  let disposed = false;
  let supersedingDial = false;
  let status: TerminalConnectionStatus = "connecting";
  let lastDialSeeded = true;
  const clientId = createTerminalClientId();
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
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.addEventListener("load", () => resolve(reader.result as ArrayBuffer));
      reader.addEventListener("error", () => reject(reader.error));
      reader.readAsArrayBuffer(blob);
    });
  };

  const bytesFromBinaryDataSync = (data: unknown): Uint8Array | null => {
    if (data instanceof ArrayBuffer) return new Uint8Array(data);
    if (ArrayBuffer.isView(data)) {
      return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
    }
    if (
      data != null &&
      typeof data === "object" &&
      Object.prototype.toString.call(data) === "[object ArrayBuffer]"
    ) {
      return new Uint8Array(data as ArrayBuffer);
    }
    return null;
  };

  const bytesFromBinaryBlob = async (blob: Blob): Promise<Uint8Array> => {
    return new Uint8Array(await readBlobArrayBuffer(blob));
  };

  const handleJsonControlFrame = (text: string): boolean => {
    let payload: { type?: string; data?: string; error?: unknown };
    try {
      payload = JSON.parse(text) as { type?: string; data?: string; error?: unknown };
    } catch {
      return false;
    }
    if (payload.type === "output" && payload.data) {
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

  const onSocketMessage = (event: MessageEvent) => {
    const raw = event.data;

    if (typeof raw === "string") {
      if (!handleJsonControlFrame(raw)) {
        events.onOutput(raw);
      }
      return;
    }

    const syncBytes = bytesFromBinaryDataSync(raw);
    if (syncBytes) {
      if (syncBytes.length > 0 && syncBytes[0] === 0x7b ) {
        const asText = new TextDecoder().decode(syncBytes);
        if (handleJsonControlFrame(asText)) return;
      }
      events.onOutput(outputDecoder.decode(syncBytes, { stream: true }));
      return;
    }

    void (async () => {
      if (!(raw instanceof Blob)) {
        events.onOutput(String(raw));
        return;
      }
      const binaryBytes = await bytesFromBinaryBlob(raw);
      if (binaryBytes.length > 0 && binaryBytes[0] === 0x7b ) {
        const asText = new TextDecoder().decode(binaryBytes);
        if (handleJsonControlFrame(asText)) return;
      }
      events.onOutput(outputDecoder.decode(binaryBytes, { stream: true }));
    })();
  };

  const scheduleReconnect = (stableOpen = false) => {
    if (disposed) return;
    setStatus("reconnecting");
    const immediateAfterOpen =
      stableOpen && document.visibilityState === "visible" && reconnectAttempts === 0;
    const delay = immediateAfterOpen
      ? 0
      : Math.min(RECONNECT_MAX_DELAY_MS, 1000 * 2 ** reconnectAttempts);
    reconnectAttempts += 1;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(() => {
      if (disposed) return;
      if (document.visibilityState !== "visible") return;
      connect(false);
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

  const reconnectNow = () => {
    unstableOpenFailures = 0;
    redialNow(true);
  };

  function connect(seedHistory: boolean) {
    lastDialSeeded = seedHistory;
    dialOpened = false;
    dialOpenedAt = undefined;
    const priorSocket = socket;
    const dialSocket = openTaskTerminalSocket(handle, seedHistory, clientId);
    socket = dialSocket;
    dialSocket.binaryType = "arraybuffer";
    if (priorSocket) {
      supersedingDial = true;
      priorSocket.close();
      supersedingDial = false;
    }
    dialSocket.addEventListener("open", () => {
      if (socket !== dialSocket) return;
      const isReconnect = everOpened;
      everOpened = true;
      dialOpened = true;
      dialOpenedAt = Date.now();
      sessionRenewTried = false;
      attachFailed = false;
      setStatus("connected");
      events.onOpen(isReconnect, lastDialSeeded);
    });
    dialSocket.addEventListener("message", onSocketMessage);
    dialSocket.addEventListener("error", () => {});
    dialSocket.addEventListener("close", () => {
      if (socket !== dialSocket) return;
      if (supersedingDial) return;
      if (disposed) return;
      if (attachFailed) {
        setStatus("unavailable");
        return;
      }
      const stableOpen =
        dialOpenedAt !== undefined && Date.now() - dialOpenedAt >= STABLE_OPEN_MS;
      if (dialOpened) {
        if (stableOpen) {
          unstableOpenFailures = 0;
          reconnectAttempts = 0;
        } else {
          unstableOpenFailures += 1;
          if (unstableOpenFailures >= IMMEDIATE_FAILURE_LIMIT) {
            setStatus("unavailable");
            return;
          }
        }
      }
      if (!dialOpened && !sessionRenewTried) {
        sessionRenewTried = true;
        setStatus("reconnecting");
        void renewBrowserSession().then(
          () => {
            if (!disposed) redialNow(lastDialSeeded);
          },
          () => {
            if (!disposed) scheduleReconnect();
          },
        );
        return;
      }
      if (!everOpened && reconnectAttempts >= IMMEDIATE_FAILURE_LIMIT) {
        setStatus("unavailable");
        return;
      }
      scheduleReconnect(stableOpen);
    });
  }

  const onVisibility = () => {
    if (document.visibilityState === "visible" && status === "reconnecting") {
      redialNow(false);
    }
  };
  document.addEventListener("visibilitychange", onVisibility);

  connect(true);

  return {
    isOpen: () => socket?.readyState === WebSocket.OPEN,
    sendInput(data: string) {
      if (!socket || socket.readyState !== WebSocket.OPEN) return;
      const MAX_INPUT_FRAME_BYTES = 4096;
      const bytes = inputEncoder.encode(data);
      for (let offset = 0; offset < bytes.byteLength; offset += MAX_INPUT_FRAME_BYTES) {
        const end = Math.min(offset + MAX_INPUT_FRAME_BYTES, bytes.byteLength);
        socket.send(bytes.subarray(offset, end));
      }
    },
    sendResize(cols: number, rows: number) {
      if (!socket || socket.readyState !== WebSocket.OPEN) return;
      socket.send(JSON.stringify({ type: "resize", cols, rows }));
    },
    reconnectNow,
    dispose() {
      disposed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      document.removeEventListener("visibilitychange", onVisibility);
      socket?.close();
      socket = undefined;
    },
  };
}
