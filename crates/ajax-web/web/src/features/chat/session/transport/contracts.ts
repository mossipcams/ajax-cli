// Authenticated orchestration-chat WebSocket transport (ACP-primary; not PTY).

import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";

export const SESSION_PROTOCOL_VERSION = 2;

/** Match host FIFO cap (`web_session::MAX_QUEUED_PROMPTS`). */
export const MAX_QUEUED_PROMPTS = 8;

/** Match the host's per-frame ceiling (`ws_bridge::MAX_SESSION_FRAME_BYTES`). */
export const MAX_FRAME_BYTES = 256 * 1024;

export const PROMPT_TOO_LONG = "That message is too long to send. Shorten it and try again.";
export const OPEN_FAILURE = "Session WebSocket failed to open";

export type SocketListener = (event: Event | MessageEvent) => void;

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

export type PendingPrompt = { text: string; clientMessageId: string };
