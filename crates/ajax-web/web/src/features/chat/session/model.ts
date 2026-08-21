/** Chat-facing session types. Presentation imports these via session/public only. */

export type ToolContent =
  | { type: "text"; text: string }
  | { type: "diff"; path: string; oldText?: string | null; newText: string };

export type ToolStatus = "pending" | "in_progress" | "completed" | "failed";

export interface ToolCall {
  callId: string;
  title: string;
  kind: string;
  status: ToolStatus;
  locations: string[];
  content: ToolContent[];
  startedAt?: number;
  endedAt?: number;
}

export interface PlanEntry {
  content: string;
  status: string;
}

export type ConversationItem =
  | { kind: "prose"; id: string; role: "user" | "agent"; text: string; messageId?: string }
  | { kind: "note"; id: string; tone: "info" | "error"; text: string }
  | { kind: "thought"; id: string; text: string; messageId?: string }
  | { kind: "tool"; id: string; call: ToolCall }
  | { kind: "plan"; id: string; entries: PlanEntry[] }
  | { kind: "permission"; id: string; requestId: string; title: string; resolved: boolean };

export interface Decision {
  requestId: string;
  title: string;
  detail: string;
}

export interface Usage {
  used: number;
  size: number;
}

export interface TurnUsage {
  requestId?: string;
  inputTokens?: number;
  outputTokens?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  totalTokens?: number;
}

export interface ChatTurnState {
  busy: boolean;
  proseOpen: boolean;
}

export interface ChatPermissionState {
  decision: Decision | null;
  resolvedIds: string[];
}

export interface ChatStatusState {
  acpState: string | null;
  detail: string | null;
}

export interface ChatUsageState {
  context: Usage | null;
  turn: TurnUsage | null;
}

import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";

/** Host-confirmed model and advertised config options from session snapshots. */
export interface ChatModelState {
  confirmedModel: string;
  configOptions?: LiveSessionConfigOption[];
}

export interface ChatSessionView {
  conversation: ConversationItem[];
  turn: ChatTurnState;
  permission: ChatPermissionState;
  status: ChatStatusState;
  usage: ChatUsageState;
  model: ChatModelState;
  revision: number;
}

/** Closed union — reducer input after wire projection. No raw wire role/status strings. */
export type ChatSessionEvent =
  | { type: "agent_message"; text: string; itemId?: string; messageId?: string }
  | { type: "user_message"; text: string; itemId?: string; messageId?: string }
  | { type: "thought_message"; text: string; itemId?: string; messageId?: string }
  | { type: "host_note"; text: string }
  | { type: "system_message"; text: string }
  | { type: "tool_call"; call: Omit<ToolCall, "startedAt" | "endedAt"> }
  | { type: "plan_update"; entries: PlanEntry[] }
  | { type: "context_usage"; used: number; size: number }
  | { type: "turn_usage"; usage: TurnUsage }
  | { type: "permission_request"; requestId: string; title: string; detail: string }
  | { type: "permission_resolved"; requestId: string }
  | { type: "acp_status"; state: string; detail?: string }
  | { type: "turn_end"; stopReason?: string }
  | { type: "session_error"; message: string }
  | { type: "session_ready"; busy?: boolean; reset?: boolean }
  | { type: "prompt_accepted" };

export type ChatSessionAction =
  | { type: "event"; event: ChatSessionEvent }
  | { type: "prompt"; text: string }
  | { type: "decided" }
  | { type: "reset" };

/** Reducer carries seq for stable item ids; not part of the public view contract. */
export interface ChatSessionReducerState {
  view: ChatSessionView;
  seq: number;
}

export const initialChatSessionView: ChatSessionView = {
  conversation: [],
  turn: { busy: false, proseOpen: true },
  permission: { decision: null, resolvedIds: [] },
  status: { acpState: null, detail: null },
  usage: { context: null, turn: null },
  model: { confirmedModel: "" },
  revision: 0,
};

export const initialChatSessionReducerState: ChatSessionReducerState = {
  view: initialChatSessionView,
  seq: 0,
};
