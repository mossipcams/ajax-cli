import { parseLiveConfigOptions } from "@/shared/lib/liveSessionConfig";
import {
  SESSION_PROTOCOL_VERSION,
  type ParsedServerFrame,
  type ToolContent,
  type WebSessionServerEvent,
} from "./contracts";

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
