// Authenticated orchestration-chat WebSocket transport (ACP-primary; not PTY).

import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import type { LiveAvailableCommand } from "@/shared/lib/liveSessionCommands";
import type { LivePromptCapabilities } from "@/shared/lib/liveSessionPromptCapabilities";
import type { PromptContentBlockWire } from "@/shared/lib/promptContent";
import type { OutputContentBlock, ToolContent } from "@/shared/lib/liveSessionOutputContent";

export const SESSION_PROTOCOL_VERSION = 2;

/** Match host FIFO cap (`web_session::MAX_QUEUED_PROMPTS`). */
export const MAX_QUEUED_PROMPTS = 8;

/** Match the host's per-frame ceiling (`ws_bridge::MAX_SESSION_FRAME_BYTES`). */
export const MAX_FRAME_BYTES = 8 * 1024 * 1024;

/** Maximum inline image blocks per prompt (mirrors host `prompt_content::MAX_IMAGE_BLOCKS`). */
export const MAX_IMAGE_BLOCKS = 8;

/** Headroom reserved for JSON framing outside base64 image payloads. */
export const PROMPT_FRAME_HEADROOM_BYTES = 4096;

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

export type { OutputContentBlock, ToolContent } from "@/shared/lib/liveSessionOutputContent";

export type WebSessionServerEvent =
  | { type: "ready"; model?: string; busy?: boolean; reset?: boolean }
  | {
      type: "message";
      role: string;
      text: string;
      contentBlocks?: OutputContentBlock[];
      itemId?: string;
      messageId?: string;
    }
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
  | {
      type: "elicitation_request";
      requestId: string;
      message: string;
      schema: unknown;
    }
  | { type: "elicitation_resolved"; requestId: string; action: string }
  | { type: "status"; state: string; detail?: string | null }
  | { type: "turn_end"; stopReason?: string | null }
  | { type: "error"; message: string };

export interface SessionSnapshot {
  type: "snapshot";
  protocolVersion: number;
  cursor: number;
  model: string;
  sessionConfigOptions?: LiveSessionConfigOption[];
  availableCommands?: LiveAvailableCommand[];
  promptCapabilities?: LivePromptCapabilities;
  sessionTitle?: string;
  turnState: "idle" | "busy";
  reset: boolean;
  pendingPermission?: {
    requestId: string;
    title?: string | null;
    detail?: string | null;
  };
  pendingElicitation?: {
    requestId: string;
    message: string;
    schema: unknown;
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
  sendPrompt(text: string, contentBlocks?: PromptContentBlockWire[]): string;
  sendCancel(keepQueue?: boolean): void;
  sendClear(): void;
  /** @deprecated Use setConfigOption for live picks. */
  setModel(model: string): void;
  setConfigOption(configId: string, value: string | boolean): void;
  respondPermission(requestId: string, approved: boolean, reason?: string): void;
  respondElicitation(
    requestId: string,
    action: "accept" | "decline" | "cancel",
    content?: Record<string, string | number | boolean | string[]>,
  ): void;
  dispose(): void;
}

export type PendingPrompt = {
  text: string;
  clientMessageId: string;
  contentBlocks?: PromptContentBlockWire[];
};
