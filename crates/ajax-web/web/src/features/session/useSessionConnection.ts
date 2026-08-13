import { useCallback, useEffect, useRef, useState } from "react";
import { openTaskSessionSocket } from "@/shared/lib/api";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import {
  explainOpenFailure,
  OPEN_FAILURE,
  type ServerEvent,
} from "./projectSession";
import { readSessionModel, writeSessionModel } from "./sessionModel";

const WS_OPEN = 1;
const WS_CLOSED = 3;

function parseServerEvent(raw: string): ServerEvent | null {
  try {
    const value = JSON.parse(raw) as unknown;
    if (!value || typeof value !== "object" || !("type" in value)) return null;
    return value as ServerEvent;
  } catch {
    return null;
  }
}

export function useSessionConnection(
  handle: string | null,
  detail: BrowserTaskDetail | null,
) {
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const [everOpened, setEverOpened] = useState(false);
  const [offline, setOffline] = useState(false);
  const socketRef = useRef<WebSocket | null>(null);
  const detailRef = useRef(detail);
  detailRef.current = detail;

  useEffect(() => {
    if (!handle) return;
    let disposed = false;
    let hadReady = false;
    const connectModel = readSessionModel();

    setEvents([]);
    setConnected(false);
    setEverOpened(false);
    setOffline(false);

    const ws = openTaskSessionSocket(handle, connectModel);
    socketRef.current = ws;

    if (ws.readyState === WS_CLOSED) {
      setOffline(true);
    }

    const onOpen = () => {
      if (disposed) return;
      setConnected(true);
      setOffline(false);
    };

    const onClose = () => {
      if (disposed) return;
      setConnected(false);
      setOffline(true);
    };

    const onMessage = (event: MessageEvent) => {
      const parsed = parseServerEvent(String(event.data));
      if (!parsed) return;

      if (parsed.type === "ready") {
        if (hadReady) {
          setEvents([]);
        }
        hadReady = true;
        setEverOpened(true);
        setConnected(true);
        setOffline(false);
        writeSessionModel(parsed.model ?? connectModel);
        return;
      }

      if (parsed.type === "error" && parsed.message === OPEN_FAILURE) {
        setEvents((prev) => [
          ...prev,
          { type: "error", message: explainOpenFailure(detailRef.current) },
        ]);
        return;
      }

      setEvents((prev) => [...prev, parsed]);
    };

    ws.addEventListener("open", onOpen);
    ws.addEventListener("close", onClose);
    ws.addEventListener("message", onMessage);

    return () => {
      disposed = true;
      ws.removeEventListener("open", onOpen);
      ws.removeEventListener("close", onClose);
      ws.removeEventListener("message", onMessage);
      ws.close();
      socketRef.current = null;
      setConnected(false);
    };
  }, [handle]);

  const send = useCallback((payload: unknown): boolean => {
    const ws = socketRef.current;
    if (!ws || ws.readyState !== WS_OPEN) return false;
    ws.send(JSON.stringify(payload));
    return true;
  }, []);

  return { events, connected, everOpened, offline, send };
}
