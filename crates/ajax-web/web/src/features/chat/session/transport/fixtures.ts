import type { WebSessionServerEvent } from "./contracts";

export const SESSION_PROTOCOL_VERSION = 2;

export function snapshotJson(
  overrides: Partial<{
    cursor: number;
    model: string;
    turnState: "idle" | "busy";
    reset: boolean;
    pendingPermission: { requestId: string; title?: string | null; detail?: string | null };
  }> = {},
): string {
  return JSON.stringify({
    type: "snapshot",
    protocolVersion: SESSION_PROTOCOL_VERSION,
    cursor: 0,
    model: "auto",
    turnState: "idle",
    reset: false,
    ...overrides,
  });
}

export function eventJson(cursor: number, event: WebSessionServerEvent): string {
  return JSON.stringify({
    type: "event",
    protocolVersion: SESSION_PROTOCOL_VERSION,
    cursor,
    payload: event,
  });
}

export const FIXTURE_COMMANDS = {
  prompt: { type: "prompt", text: "Ship it", clientMessageId: "c1" },
  cancel: { type: "cancel" },
  cancelKeepQueue: { type: "cancel", keepQueue: true },
  setModel: { type: "set_config_option", configId: "model", value: "composer-2.5" },
  permission: { type: "permission", requestId: "p1", approved: true },
} as const;

export const FIXTURE_SNAPSHOT = {
  idle: {
    type: "snapshot",
    protocolVersion: SESSION_PROTOCOL_VERSION,
    cursor: 2,
    model: "composer-2.5",
    turnState: "idle",
    reset: false,
  },
  busy: {
    type: "snapshot",
    protocolVersion: SESSION_PROTOCOL_VERSION,
    cursor: 3,
    model: "composer-2.5",
    turnState: "busy",
    reset: false,
    pendingPermission: { requestId: "p1", title: "Run tests?" },
  },
} as const;

export const FIXTURE_EVENTS = {
  agentMessage: {
    type: "message",
    role: "agent",
    text: "On it",
    itemId: "i1",
  },
  userMessage: {
    type: "message",
    role: "user",
    text: "Fix it",
    itemId: "i2",
  },
  promptAccepted: { type: "prompt_accepted", clientMessageId: "c1" },
  toolCall: {
    type: "tool_call",
    callId: "t1",
    title: "Read",
    kind: "read",
    status: "in_progress",
    locations: [],
    content: [],
  },
  turnEnd: { type: "turn_end", stopReason: "end_turn" },
  error: { type: "error", message: "boom" },
} as const;
