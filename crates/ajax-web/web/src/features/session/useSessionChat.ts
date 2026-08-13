import { useCallback, useEffect, useRef, useState } from "react";
import { openTaskSessionSocket } from "@/shared/lib/api";

export type SessionPermissionRequest = {
  requestId: string;
  title: string;
};

function parseSessionEvent(raw: string): Record<string, unknown> | null {
  try {
    const value = JSON.parse(raw) as unknown;
    return value && typeof value === "object" ? (value as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

function marksWorking(event: Record<string, unknown>): boolean {
  const type = event.type;
  if (type === "message" && event.role === "agent") return true;
  if (type === "tool_call") return true;
  if (type === "permission_request") return true;
  return false;
}

const WS_OPEN = 1;

export function useSessionChat(handle: string) {
  const socketRef = useRef<WebSocket | null>(null);
  const workingRef = useRef(false);
  const [connected, setConnected] = useState(false);
  const [working, setWorking] = useState(false);
  const [permission, setPermission] = useState<SessionPermissionRequest | null>(null);

  const setWorkingState = (next: boolean) => {
    workingRef.current = next;
    setWorking(next);
  };

  useEffect(() => {
    const ws = openTaskSessionSocket(handle);
    socketRef.current = ws;

    const onOpen = () => setConnected(true);
    const onClose = () => setConnected(false);
    const onMessage = (event: MessageEvent<string>) => {
      const payload = parseSessionEvent(String(event.data));
      if (!payload) return;
      if (payload.type === "turn_end") {
        setWorkingState(false);
        return;
      }
      if (payload.type === "permission_resolved") {
        setPermission(null);
        return;
      }
      if (payload.type === "permission_request") {
        setWorkingState(true);
        setPermission({
          requestId: String(payload.requestId ?? ""),
          title: String(payload.title ?? "Permission required"),
        });
        return;
      }
      if (marksWorking(payload)) {
        setWorkingState(true);
      }
    };

    ws.addEventListener("open", onOpen);
    ws.addEventListener("close", onClose);
    ws.addEventListener("message", onMessage);
    if (ws.readyState === 3) {
      setConnected(false);
    }

    return () => {
      ws.removeEventListener("open", onOpen);
      ws.removeEventListener("close", onClose);
      ws.removeEventListener("message", onMessage);
      ws.close();
      socketRef.current = null;
    };
  }, [handle]);

  const sendJson = useCallback((payload: unknown): boolean => {
    const ws = socketRef.current;
    if (!ws || ws.readyState !== WS_OPEN) return false;
    ws.send(JSON.stringify(payload));
    return true;
  }, []);

  const sendPrompt = useCallback((text: string) => sendJson({ type: "prompt", text }), [sendJson]);

  const sendCancelKeepQueue = useCallback(() => sendJson({ type: "cancel", keepQueue: true }), [sendJson]);

  const sendPermission = useCallback(
    (approved: boolean) => {
      if (!permission) return false;
      return sendJson({
        type: "permission",
        requestId: permission.requestId,
        approved,
      });
    },
    [permission, sendJson],
  );

  return {
    connected,
    working,
    workingRef,
    permission,
    sendPrompt,
    sendCancelKeepQueue,
    sendPermission,
  };
}
