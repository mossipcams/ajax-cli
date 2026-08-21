import type { SessionSnapshot, WebSessionServerEvent } from "./transport/contracts";
import type { ChatSessionEvent, PlanEntry, ToolStatus, TurnUsage } from "./model";

const TOOL_STATUSES: ToolStatus[] = ["pending", "in_progress", "completed", "failed"];

function toolStatus(raw: string): ToolStatus {
  const value = raw.toLowerCase();
  return (TOOL_STATUSES as string[]).includes(value) ? (value as ToolStatus) : "in_progress";
}

function projectServerEvent(event: WebSessionServerEvent): ChatSessionEvent | null {
  switch (event.type) {
    case "message": {
      const contentBlocks = event.contentBlocks?.length ? event.contentBlocks : undefined;
      if (!event.text && !contentBlocks?.length) return null;
      if (event.role === "thought") {
        return {
          type: "thought_message",
          text: event.text,
          ...(contentBlocks ? { contentBlocks } : {}),
          itemId: event.itemId,
          messageId: event.messageId,
        };
      }
      if (event.role === "agent") {
        return {
          type: "agent_message",
          text: event.text,
          ...(contentBlocks ? { contentBlocks } : {}),
          itemId: event.itemId,
          messageId: event.messageId,
        };
      }
      if (event.role === "user") {
        return {
          type: "user_message",
          text: event.text,
          ...(contentBlocks ? { contentBlocks } : {}),
          itemId: event.itemId,
          messageId: event.messageId,
        };
      }
      if (event.role === "note") {
        return { type: "host_note", text: event.text };
      }
      return { type: "system_message", text: event.text };
    }
    case "prompt_accepted":
      return { type: "prompt_accepted" };
    case "tool_call":
      return {
        type: "tool_call",
        call: {
          callId: event.callId,
          title: event.title,
          kind: event.kind,
          status: toolStatus(event.status),
          locations: event.locations ?? [],
          content: event.content ?? [],
        },
      };
    case "plan":
      return { type: "plan_update", entries: event.entries as PlanEntry[] };
    case "usage":
      return { type: "context_usage", used: event.used, size: event.size };
    case "turn_usage": {
      const usage: TurnUsage = {};
      if (event.requestId !== undefined) usage.requestId = event.requestId;
      if (event.inputTokens !== undefined) usage.inputTokens = event.inputTokens;
      if (event.outputTokens !== undefined) usage.outputTokens = event.outputTokens;
      if (event.cacheReadTokens !== undefined) usage.cacheReadTokens = event.cacheReadTokens;
      if (event.cacheWriteTokens !== undefined) usage.cacheWriteTokens = event.cacheWriteTokens;
      if (event.totalTokens !== undefined) usage.totalTokens = event.totalTokens;
      if (Object.keys(usage).length === 0) return null;
      return { type: "turn_usage", usage };
    }
    case "permission_request":
      return {
        type: "permission_request",
        requestId: event.requestId,
        title: event.title?.trim() || "Permission required",
        detail: event.detail?.trim() || "",
      };
    case "permission_resolved":
      return { type: "permission_resolved", requestId: event.requestId };
    case "elicitation_request":
      return {
        type: "elicitation_request",
        requestId: event.requestId,
        message: event.message,
        schema: event.schema,
      };
    case "elicitation_resolved":
      return {
        type: "elicitation_resolved",
        requestId: event.requestId,
        action: event.action,
      };
    case "status":
      return {
        type: "acp_status",
        state: event.state.trim(),
        detail: event.detail?.trim() || undefined,
      };
    case "turn_end":
      return {
        type: "turn_end",
        stopReason: event.stopReason != null ? event.stopReason : undefined,
      };
    case "artifact":
      return null;
    case "error":
      return { type: "session_error", message: event.message };
    case "ready":
      return { type: "session_ready", busy: event.busy, reset: event.reset };
  }
}

/** Map validated wire input to typed Chat events. Raw transport stops here. */
export function projectWireInput(
  input: SessionSnapshot | WebSessionServerEvent,
): ChatSessionEvent | ChatSessionEvent[] | null {
  if ("type" in input && input.type === "snapshot") {
    const events: ChatSessionEvent[] = [];
    if (input.reset) {
      events.push({ type: "session_ready", reset: true, busy: input.turnState === "busy" });
    }
    if (input.pendingPermission) {
      events.push({
        type: "permission_request",
        requestId: input.pendingPermission.requestId,
        title: input.pendingPermission.title?.trim() || "Permission required",
        detail: input.pendingPermission.detail?.trim() || "",
      });
    }
    if (input.pendingElicitation) {
      events.push({
        type: "elicitation_request",
        requestId: input.pendingElicitation.requestId,
        message: input.pendingElicitation.message,
        schema: input.pendingElicitation.schema,
      });
    }
    return events.length ? events : null;
  }
  return projectServerEvent(input);
}

/** Convenience for transport callbacks that receive one server event at a time. */
export function projectWireEvent(event: WebSessionServerEvent): ChatSessionEvent | null {
  return projectServerEvent(event);
}
