// Authenticated orchestration-chat WebSocket transport (ACP-primary; not PTY).

import type { LiveSessionConfigOption } from "./liveSessionConfig";
import { parseLiveConfigOptions } from "./liveSessionConfig";

const OPEN_READY_STATE = 1;
export const SESSION_PROTOCOL_VERSION = 2;

/** Match host FIFO cap (`web_session::MAX_QUEUED_PROMPTS`). */
const MAX_QUEUED_PROMPTS = 8;

/** Match the host's per-frame ceiling (`ws_bridge::MAX_SESSION_FRAME_BYTES`). */
export const MAX_FRAME_BYTES = 256 * 1024;

export const PROMPT_TOO_LONG = "That message is too long to send. Shorten it and try again.";
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

export type ToolContent =
  | { type: "text"; text: string }
  | { type: "diff"; path: string; oldText?: string | null; newText: string };

export type WebSessionServerEvent =
  | { type: "ready"; model?: string; busy?: boolean; reset?: boolean }
  | { type: "message"; role: string; text: string; itemId?: string; messageId?: string }
  | { type: "prompt_accepted"; clientMessageId: string }
  | { type: "artifact"; kind: string; title?: string | null; body?: string | null }
  | {
      type: "tool_call";
      callId: string;
      title: string;
      kind: string;
      status: string;
      locations?: string[];
      content?: ToolContent[];
    }
  | { type: "plan"; entries: { content: string; status: string }[] }
  | { type: "usage"; used: number; size: number }
  | {
      type: "turn_usage";
      requestId?: string;
      inputTokens?: number;
      outputTokens?: number;
      cacheReadTokens?: number;
      cacheWriteTokens?: number;
      totalTokens?: number;
    }
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

export interface SessionSnapshot {
  type: "snapshot";
  protocolVersion: number;
  cursor: number;
  model: string;
  sessionConfigOptions?: LiveSessionConfigOption[];
  turnState: "idle" | "busy";
  reset: boolean;
  pendingPermission?: {
    requestId: string;
    title?: string | null;
    detail?: string | null;
  };
}

export type ParsedServerFrame =
  | { kind: "snapshot"; snapshot: SessionSnapshot }
  | { kind: "event"; cursor: number; event: WebSessionServerEvent };

export interface WebSessionTransportCallbacks {
  onReady: (model: string) => void;
  onEvent: (event: WebSessionServerEvent) => void;
  /** Host snapshot refresh (initial attach or applied config/model change). */
  onSnapshot?: (snapshot: SessionSnapshot) => void;
  /** Next event cursor to request on an in-page reconnect (not persisted). */
  onCursorAdvance?: (nextToRead: number) => void;
  onClosed: () => void;
}

export interface WebSessionTransport {
  sendPrompt(text: string): string;
  sendCancel(keepQueue?: boolean): void;
  /** @deprecated Use setConfigOption for live picks. */
  setModel(model: string): void;
  setConfigOption(configId: string, value: string | boolean): void;
  respondPermission(requestId: string, approved: boolean, reason?: string): void;
  dispose(): void;
}

function sessionSocketUrl(handle: string, model?: string, cursor?: number): string {
  const protocol =
    typeof location !== "undefined" && location.protocol === "https:" ? "wss:" : "ws:";
  const host = typeof location !== "undefined" ? location.host : "localhost";
  const base = `${protocol}//${host}/api/tasks/${encodeURIComponent(handle)}/session`;
  const params = new URLSearchParams();
  if (model) params.set("model", model);
  if (cursor !== undefined) params.set("cursor", String(cursor));
  const qs = params.toString();
  return qs ? `${base}?${qs}` : base;
}

type PendingPrompt = { text: string; clientMessageId: string };

function promptFrame(prompt: PendingPrompt): string {
  return JSON.stringify({
    type: "prompt",
    text: prompt.text,
    clientMessageId: prompt.clientMessageId,
  });
}

function frameFits(prompt: PendingPrompt): boolean {
  return new TextEncoder().encode(promptFrame(prompt)).length <= MAX_FRAME_BYTES;
}

function outboxKey(handle: string): string {
  return `ajax.web.session.outbox.${encodeURIComponent(handle)}`;
}

function cursorKey(handle: string): string {
  return `ajax.web.session.cursor.${encodeURIComponent(handle)}`;
}

function readOutbox(handle: string): PendingPrompt[] {
  try {
    const raw = sessionStorage.getItem(outboxKey(handle));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (item): item is PendingPrompt =>
        !!item &&
        typeof item === "object" &&
        typeof (item as PendingPrompt).text === "string" &&
        typeof (item as PendingPrompt).clientMessageId === "string",
    );
  } catch {
    return [];
  }
}

function writeOutbox(handle: string, pending: PendingPrompt[]): void {
  try {
    if (pending.length) sessionStorage.setItem(outboxKey(handle), JSON.stringify(pending));
    else sessionStorage.removeItem(outboxKey(handle));
  } catch {
    // Private mode / storage denied: the live socket still works.
  }
}

export function readSessionCursor(handle: string): number | undefined {
  try {
    const raw = sessionStorage.getItem(cursorKey(handle));
    if (!raw) return undefined;
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : undefined;
  } catch {
    return undefined;
  }
}

export function writeSessionCursor(handle: string, cursor: number): void {
  try {
    sessionStorage.setItem(cursorKey(handle), String(cursor));
  } catch {
    // ignore storage failures
  }
}

export function clearSessionCursor(handle: string): void {
  try {
    sessionStorage.removeItem(cursorKey(handle));
  } catch {
    // ignore
  }
}

function newPromptId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object";
}

function optionalTokenField(
  payload: Record<string, unknown>,
  key: string,
): number | undefined {
  const value = payload[key];
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function parsePayload(payload: Record<string, unknown>): WebSessionServerEvent | null {
  if (typeof payload.type !== "string") return null;
  switch (payload.type) {
    case "ready":
      return {
        type: "ready",
        ...(typeof payload.model === "string" ? { model: payload.model } : {}),
        ...(typeof payload.busy === "boolean" ? { busy: payload.busy } : {}),
      };
    case "message": {
      if (typeof payload.role !== "string" || typeof payload.text !== "string") return null;
      return {
        type: "message",
        role: payload.role,
        text: payload.text,
        ...(typeof payload.itemId === "string" ? { itemId: payload.itemId } : {}),
        ...(typeof payload.messageId === "string" ? { messageId: payload.messageId } : {}),
      };
    }
    case "prompt_accepted":
      if (typeof payload.clientMessageId !== "string") return null;
      return { type: "prompt_accepted", clientMessageId: payload.clientMessageId };
    case "artifact":
      if (typeof payload.kind !== "string") return null;
      return {
        type: "artifact",
        kind: payload.kind,
        ...(payload.title === null || typeof payload.title === "string"
          ? { title: payload.title as string | null }
          : {}),
        ...(payload.body === null || typeof payload.body === "string"
          ? { body: payload.body as string | null }
          : {}),
      };
    case "tool_call": {
      if (
        typeof payload.callId !== "string" ||
        typeof payload.title !== "string" ||
        typeof payload.kind !== "string" ||
        typeof payload.status !== "string"
      ) {
        return null;
      }
      return {
        type: "tool_call",
        callId: payload.callId,
        title: payload.title,
        kind: payload.kind,
        status: payload.status,
        ...(Array.isArray(payload.locations)
          ? { locations: payload.locations.filter((l): l is string => typeof l === "string") }
          : {}),
        ...(Array.isArray(payload.content) ? { content: payload.content as ToolContent[] } : {}),
      };
    }
    case "plan": {
      if (!Array.isArray(payload.entries)) return null;
      const entries = payload.entries.filter(
        (entry): entry is { content: string; status: string } =>
          isRecord(entry) &&
          typeof entry.content === "string" &&
          typeof entry.status === "string",
      );
      if (entries.length !== payload.entries.length) return null;
      return { type: "plan", entries };
    }
    case "usage":
      if (typeof payload.used !== "number" || typeof payload.size !== "number") return null;
      return { type: "usage", used: payload.used, size: payload.size };
    case "turn_usage": {
      const requestId =
        typeof payload.requestId === "string" ? payload.requestId : undefined;
      const inputTokens = optionalTokenField(payload, "inputTokens");
      const outputTokens = optionalTokenField(payload, "outputTokens");
      const cacheReadTokens = optionalTokenField(payload, "cacheReadTokens");
      const cacheWriteTokens = optionalTokenField(payload, "cacheWriteTokens");
      const totalTokens = optionalTokenField(payload, "totalTokens");
      if (
        requestId === undefined &&
        inputTokens === undefined &&
        outputTokens === undefined &&
        cacheReadTokens === undefined &&
        cacheWriteTokens === undefined &&
        totalTokens === undefined
      ) {
        return null;
      }
      return {
        type: "turn_usage",
        ...(requestId !== undefined ? { requestId } : {}),
        ...(inputTokens !== undefined ? { inputTokens } : {}),
        ...(outputTokens !== undefined ? { outputTokens } : {}),
        ...(cacheReadTokens !== undefined ? { cacheReadTokens } : {}),
        ...(cacheWriteTokens !== undefined ? { cacheWriteTokens } : {}),
        ...(totalTokens !== undefined ? { totalTokens } : {}),
      };
    }
    case "permission_request":
      if (typeof payload.requestId !== "string") return null;
      return {
        type: "permission_request",
        requestId: payload.requestId,
        ...(payload.title === null || typeof payload.title === "string"
          ? { title: payload.title as string | null }
          : {}),
        ...(payload.detail === null || typeof payload.detail === "string"
          ? { detail: payload.detail as string | null }
          : {}),
      };
    case "permission_resolved":
      if (typeof payload.requestId !== "string" || typeof payload.approved !== "boolean") {
        return null;
      }
      return {
        type: "permission_resolved",
        requestId: payload.requestId,
        approved: payload.approved,
      };
    case "status":
      if (typeof payload.state !== "string") return null;
      return {
        type: "status",
        state: payload.state,
        ...(payload.detail === null || typeof payload.detail === "string"
          ? { detail: payload.detail as string | null }
          : {}),
      };
    case "turn_end":
      return {
        type: "turn_end",
        ...(payload.stopReason === null || typeof payload.stopReason === "string"
          ? { stopReason: payload.stopReason as string | null }
          : {}),
      };
    case "error":
      if (typeof payload.message !== "string") return null;
      return { type: "error", message: payload.message };
    default:
      return null;
  }
}

/** Validate protocol v2 frames at the WebSocket boundary. */
export function parseServerFrame(raw: string): ParsedServerFrame | null {
  try {
    const payload = JSON.parse(raw) as unknown;
    if (!isRecord(payload) || typeof payload.type !== "string") return null;

    if (payload.type === "snapshot") {
      if (
        payload.protocolVersion !== SESSION_PROTOCOL_VERSION ||
        typeof payload.cursor !== "number" ||
        typeof payload.model !== "string" ||
        typeof payload.reset !== "boolean" ||
        (payload.turnState !== "idle" && payload.turnState !== "busy")
      ) {
        return null;
      }
      const pending =
        payload.pendingPermission === undefined
          ? undefined
          : isRecord(payload.pendingPermission) &&
              typeof payload.pendingPermission.requestId === "string"
            ? {
                requestId: payload.pendingPermission.requestId,
                ...(payload.pendingPermission.title === null ||
                typeof payload.pendingPermission.title === "string"
                  ? { title: payload.pendingPermission.title as string | null }
                  : {}),
                ...(payload.pendingPermission.detail === null ||
                typeof payload.pendingPermission.detail === "string"
                  ? { detail: payload.pendingPermission.detail as string | null }
                  : {}),
              }
            : null;
      if (payload.pendingPermission !== undefined && !pending) return null;
      const sessionConfigOptions = parseLiveConfigOptions(payload.sessionConfigOptions);
      return {
        kind: "snapshot",
        snapshot: {
          type: "snapshot",
          protocolVersion: SESSION_PROTOCOL_VERSION,
          cursor: payload.cursor,
          model: payload.model,
          turnState: payload.turnState,
          reset: payload.reset,
          ...(sessionConfigOptions ? { sessionConfigOptions } : {}),
          ...(pending ? { pendingPermission: pending } : {}),
        },
      };
    }

    if (payload.type === "event") {
      if (
        payload.protocolVersion !== SESSION_PROTOCOL_VERSION ||
        typeof payload.cursor !== "number" ||
        !isRecord(payload.payload)
      ) {
        return null;
      }
      const event = parsePayload(payload.payload);
      if (!event) return null;
      return { kind: "event", cursor: payload.cursor, event };
    }

    const legacy = parsePayload(payload);
    return legacy ? { kind: "event", cursor: 0, event: legacy } : null;
  } catch {
    return null;
  }
}

/** @deprecated use parseServerFrame */
export function parseServerEvent(raw: string): WebSessionServerEvent | null {
  const frame = parseServerFrame(raw);
  if (!frame) return null;
  if (frame.kind === "snapshot") {
    return {
      type: "ready",
      model: frame.snapshot.model,
      busy: frame.snapshot.turnState === "busy",
      reset: frame.snapshot.reset,
    };
  }
  return frame.event;
}

export function clearSessionOutbox(handle: string): void {
  writeOutbox(handle, []);
}

export function clearSessionTransportState(handle: string): void {
  clearSessionOutbox(handle);
  clearSessionCursor(handle);
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
  model?: string,
  resumeCursor?: number,
): WebSessionTransport {
  let socket: WebSessionSocket | undefined;
  let ready = false;
  let disposed = false;
  let replayEndCursor: number | undefined;
  let replaySettledEvent: WebSessionServerEvent | undefined;
  const stored = readOutbox(handle);
  const pendingPrompts = stored.filter(frameFits);
  if (pendingPrompts.length !== stored.length) writeOutbox(handle, pendingPrompts);
  // ponytail: cursor is in-memory only; drop any legacy sessionStorage value on attach.
  clearSessionCursor(handle);

  const messageListener: SocketListener = (event) => {
    const messageEvent = event as MessageEvent;
    if (typeof messageEvent.data !== "string") return;
    const frame = parseServerFrame(messageEvent.data);
    if (!frame) return;

    if (frame.kind === "snapshot") {
      callbacks.onSnapshot?.(frame.snapshot);
      if (!ready) {
        ready = true;
        replayEndCursor = frame.snapshot.cursor;
        const readyEvent: WebSessionServerEvent = {
          type: "ready",
          model: frame.snapshot.model,
          busy: frame.snapshot.turnState === "busy",
          reset: frame.snapshot.reset,
        };
        replaySettledEvent = { ...readyEvent, reset: false };
        callbacks.onEvent(readyEvent);
        if (frame.snapshot.pendingPermission) {
          const pending = frame.snapshot.pendingPermission;
          callbacks.onEvent({
            type: "permission_request",
            requestId: pending.requestId,
            ...(pending.title !== undefined ? { title: pending.title } : {}),
            ...(pending.detail !== undefined ? { detail: pending.detail } : {}),
          });
        }
        callbacks.onReady(frame.snapshot.model.trim() || "auto");
        for (const prompt of pendingPrompts) sendPromptNow(prompt);
      } else {
        callbacks.onReady(frame.snapshot.model.trim() || "auto");
      }
      return;
    }

    callbacks.onCursorAdvance?.(frame.cursor + 1);
    const parsed = frame.event;
    if (parsed.type === "prompt_accepted") {
      const index = pendingPrompts.findIndex(
        (prompt) => prompt.clientMessageId === parsed.clientMessageId,
      );
      if (index >= 0) {
        pendingPrompts.splice(index, 1);
        writeOutbox(handle, pendingPrompts);
      }
    }
    callbacks.onEvent(parsed);
    if (replayEndCursor !== undefined && frame.cursor + 1 === replayEndCursor) {
      const settled = replaySettledEvent;
      replayEndCursor = undefined;
      replaySettledEvent = undefined;
      if (settled) callbacks.onEvent(settled);
    } else if (replayEndCursor !== undefined && frame.cursor >= replayEndCursor) {
      replayEndCursor = undefined;
      replaySettledEvent = undefined;
    }
  };

  const closeListener: SocketListener = () => {
    if (!disposed) callbacks.onClosed();
  };

  function sendJson(payload: Record<string, unknown>) {
    if (!socket || socket.readyState !== OPEN_READY_STATE) return;
    socket.send(JSON.stringify(payload));
  }

  function sendPromptNow(prompt: PendingPrompt) {
    sendJson({ type: "prompt", text: prompt.text, clientMessageId: prompt.clientMessageId });
  }

  socket = platform.openSocket(sessionSocketUrl(handle, model, resumeCursor));
  socket.addEventListener("message", messageListener);
  socket.addEventListener("close", closeListener);
  void waitForSocketOpen(socket).catch(() => {
    if (disposed) return;
    pendingPrompts.length = 0;
    callbacks.onEvent({ type: "error", message: OPEN_FAILURE });
  });

  return {
    sendPrompt(text) {
      const trimmed = text.trim();
      if (!trimmed) return "";
      const prompt = { text: trimmed, clientMessageId: newPromptId() };
      if (!frameFits(prompt)) {
        callbacks.onEvent({ type: "error", message: PROMPT_TOO_LONG });
        return "";
      }
      if (pendingPrompts.length >= MAX_QUEUED_PROMPTS) {
        pendingPrompts.shift();
      }
      pendingPrompts.push(prompt);
      writeOutbox(handle, pendingPrompts);
      if (ready) sendPromptNow(prompt);
      return prompt.clientMessageId;
    },
    sendCancel(keepQueue = false) {
      if (!keepQueue) {
        pendingPrompts.splice(0, pendingPrompts.length);
        writeOutbox(handle, pendingPrompts);
      }
      sendJson(keepQueue ? { type: "cancel", keepQueue: true } : { type: "cancel" });
    },
    setModel(nextModel) {
      const trimmed = nextModel.trim() || "auto";
      sendJson({ type: "set_model", model: trimmed });
    },
    setConfigOption(configId, value) {
      sendJson({
        type: "set_config_option",
        configId,
        value,
      });
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
