import type { OutputContentBlock, ToolContent } from "@/shared/lib/liveSessionOutputContent";

export type { OutputContentBlock, ToolContent };

export type ToolStatus = "pending" | "in_progress" | "completed" | "failed" | "cancelled";

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
  | {
      kind: "prose";
      id: string;
      role: "user" | "agent";
      text: string;
      contentBlocks?: OutputContentBlock[];
      messageId?: string;
      /** Browser-assigned id of an optimistic prompt; present only on optimistic prompts. */
      clientMessageId?: string;
      /** True until the host accepts the optimistic prompt with its clientMessageId. */
      pending?: boolean;
    }
  | { kind: "note"; id: string; tone: "info" | "error"; text: string }
  | {
      kind: "thought";
      id: string;
      text: string;
      contentBlocks?: OutputContentBlock[];
      messageId?: string;
    }
  | { kind: "tool"; id: string; call: ToolCall }
  | { kind: "plan"; id: string; entries: PlanEntry[] }
  | { kind: "permission"; id: string; requestId: string; title: string; resolved: boolean }
  | { kind: "elicitation"; id: string; requestId: string; message: string; resolved: boolean };

export interface Decision {
  requestId: string;
  title: string;
  detail: string;
}

export interface ElicitationDecision {
  requestId: string;
  message: string;
  schema: unknown;
  fields: import("@/shared/lib/liveSessionElicitation").ElicitationFormField[];
}

export interface Usage {
  used: number;
  size: number;
}

export interface ChatTurnState {
  busy: boolean;
  proseOpen: boolean;
}

export interface ChatPermissionState {
  decision: Decision | null;
  resolvedIds: string[];
}

export interface ChatElicitationState {
  decision: ElicitationDecision | null;
  resolvedIds: string[];
}

export interface ChatUsageState {
  context: Usage | null;
}

import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import type { LiveAvailableCommand } from "@/shared/lib/liveSessionCommands";
import type { LivePromptCapabilities } from "@/shared/lib/liveSessionPromptCapabilities";

export interface ChatModelState {
  confirmedModel: string;
  configOptions?: LiveSessionConfigOption[];
  availableCommands?: LiveAvailableCommand[];
  promptCapabilities?: LivePromptCapabilities;
}

export interface ChatSessionView {
  conversation: ConversationItem[];
  /** Earliest loaded event cursor and the server retention floor; hasEarlierOnServer is `from > floor`. */
  serverHistory?: { from: number; floor: number };
  turn: ChatTurnState;
  permission: ChatPermissionState;
  elicitation: ChatElicitationState;
  usage: ChatUsageState;
  model: ChatModelState;
  /** Host-reported transcript durability failure; present only while set. */
  transcriptError?: string;
  revision: number;
}

export type ChatSessionEvent =
  | {
      type: "agent_message";
      text: string;
      contentBlocks?: OutputContentBlock[];
      itemId?: string;
      messageId?: string;
    }
  | {
      type: "user_message";
      text: string;
      contentBlocks?: OutputContentBlock[];
      itemId?: string;
      messageId?: string;
    }
  | {
      type: "thought_message";
      text: string;
      contentBlocks?: OutputContentBlock[];
      itemId?: string;
      messageId?: string;
    }
  | { type: "host_note"; text: string }
  | { type: "system_message"; text: string }
  | { type: "tool_call"; call: Omit<ToolCall, "startedAt" | "endedAt"> }
  | { type: "plan_update"; entries: PlanEntry[] }
  | { type: "context_usage"; used: number; size: number }
  | { type: "usage_reset" }
  | { type: "permission_request"; requestId: string; title: string; detail: string }
  | { type: "permission_resolved"; requestId: string }
  | { type: "elicitation_request"; requestId: string; message: string; schema: unknown }
  | { type: "elicitation_resolved"; requestId: string; action: string }
  | { type: "turn_end"; stopReason?: string }
  | { type: "session_error"; message: string }
  | { type: "session_ready"; busy?: boolean; reset?: boolean }
  | { type: "transcript_snapshot"; transcriptError?: string }
  | { type: "prompt_accepted"; clientMessageId: string }
  | { type: "history_bounds"; from: number; floor: number }
  | { type: "prepend"; events: ChatSessionEvent[]; from: number };

export type ChatSessionAction =
  | { type: "event"; event: ChatSessionEvent }
  | { type: "events"; events: ChatSessionEvent[] }
  | { type: "prompt"; text: string; clientMessageId?: string }
  | { type: "decided" }
  | { type: "elicitation_answered" }
  | { type: "reset" };

export interface ChatSessionReducerState {
  view: ChatSessionView;
  seq: number;
}

export const initialChatSessionView: ChatSessionView = {
  conversation: [],
  turn: { busy: false, proseOpen: true },
  permission: { decision: null, resolvedIds: [] },
  elicitation: { decision: null, resolvedIds: [] },
  usage: { context: null },
  model: { confirmedModel: "" },
  revision: 0,
};

export const initialChatSessionReducerState: ChatSessionReducerState = {
  view: initialChatSessionView,
  seq: 0,
};
