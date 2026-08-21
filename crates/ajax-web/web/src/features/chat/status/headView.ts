import type { ChatSessionView, Decision, ToolCall } from "../session/public";

export type HeadState = "decision" | "working" | "attention" | "idle";

/** Narrow task-attention input — no BrowserTaskDetail in status presentation. */
export interface ChatTaskAttention {
  status: "waiting" | "error";
  explanation?: string | null;
}

export interface ChatHeadToolRow {
  callId: string;
  kind: string;
  tone: string;
  mark: string;
  title: string;
  path: string | null;
  statusLabel: string;
}

export interface ChatHeadView {
  state: HeadState;
  tone: string;
  connected: boolean;
  activityAgeMs: number;
  decision: Decision | null;
  tool: ChatHeadToolRow | null;
  planStep: string | null;
  thoughtSnippet: string | null;
  usage: ChatSessionView["usage"]["context"];
  turnUsage: ChatSessionView["usage"]["turn"];
  taskAttention: ChatTaskAttention | null;
  attentionText: string | null;
  showHeadLine: boolean;
}

const STATE_LABELS: Record<HeadState, string> = {
  decision: "Needs you",
  working: "Working",
  attention: "Needs you",
  idle: "Ready",
};

export function headStateLabel(state: HeadState, quiet: boolean): string {
  if (state === "working" && quiet) return "No recent activity";
  return STATE_LABELS[state];
}

const TOOL_TONES: Record<string, string> = {
  read: "muted",
  edit: "running",
  delete: "error",
  move: "running",
  search: "muted",
  execute: "running",
  think: "muted",
  fetch: "muted",
};

const TOOL_MARKS: Record<string, string> = {
  read: "◦",
  edit: "±",
  delete: "×",
  move: "→",
  search: "⌕",
  execute: "$",
  think: "∴",
  fetch: "↓",
  switch_mode: "⇄",
};

const TOOL_STATUS_LABELS: Record<string, string> = {
  pending: "queued",
  in_progress: "running",
  completed: "done",
  failed: "failed",
};

function toolMark(kind: string): string {
  return TOOL_MARKS[kind] ?? "•";
}

function toolStatusLabel(status: string): string {
  return TOOL_STATUS_LABELS[status] ?? status;
}

function cleanTitle(title: string): string {
  return title.replace(/`/g, "").trim();
}

function shortPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 2) return parts.join("/");
  return `…/${parts.slice(-2).join("/")}`;
}

export function mapHeadToolRow(call: ToolCall): ChatHeadToolRow {
  const location = call.locations[0];
  return {
    callId: call.callId,
    kind: call.kind || "other",
    tone: TOOL_TONES[call.kind] ?? "muted",
    mark: toolMark(call.kind),
    title: cleanTitle(call.title) || call.callId,
    path: location ? shortPath(location) : null,
    statusLabel: toolStatusLabel(call.status),
  };
}

function agentNeedsYou(status: string | null): boolean {
  const token = status?.trim().toLowerCase();
  return token === "waiting" || token === "requires_action";
}

function agentWorking(status: string | null): boolean {
  return status?.trim().toLowerCase() === "running";
}

export function headState(
  decision: Decision | null,
  busy: boolean,
  taskAttention: ChatTaskAttention | null,
  agentStatus: string | null,
): HeadState {
  if (decision) return "decision";
  if (agentNeedsYou(agentStatus)) return "attention";
  if (agentWorking(agentStatus) || busy) return "working";
  if (taskAttention) return "attention";
  return "idle";
}

export function headTone(state: HeadState, taskAttention: ChatTaskAttention | null): string {
  if (state === "decision") return "waiting";
  if (state === "working") return "running";
  if (state === "attention") return taskAttention?.status === "error" ? "error" : "waiting";
  return "idle";
}

export function isTaskLevelAttention(
  state: HeadState,
  taskAttention: ChatTaskAttention | null,
  decision: Decision | null,
): boolean {
  if (state !== "attention" || !taskAttention || decision) return false;
  return taskAttention.status === "waiting" || taskAttention.status === "error";
}

function attentionText(taskAttention: ChatTaskAttention): string {
  return (
    taskAttention.explanation?.trim() ||
    (taskAttention.status === "error" ? "The task reported an error." : "Waiting for you.")
  );
}

export function buildHeadView(input: {
  session: ChatSessionView;
  taskAttention: ChatTaskAttention | null;
  tool: ToolCall | null;
  planStep: string | null;
  thoughtSnippet: string | null;
  activityAgeMs: number;
  connected: boolean;
}): ChatHeadView {
  const { session, taskAttention, tool, planStep, thoughtSnippet, activityAgeMs, connected } =
    input;
  const decision = session.permission.decision;
  const state = headState(
    decision,
    session.turn.busy,
    taskAttention,
    session.status.acpState,
  );
  const tone = headTone(state, taskAttention);
  const taskLevel = isTaskLevelAttention(state, taskAttention, decision);
  return {
    state,
    tone,
    connected,
    activityAgeMs: state === "working" ? activityAgeMs : 0,
    decision,
    tool: tool ? mapHeadToolRow(tool) : null,
    planStep,
    thoughtSnippet,
    usage: session.usage.context,
    turnUsage: session.usage.turn,
    taskAttention: taskLevel ? taskAttention : null,
    attentionText: taskLevel && taskAttention ? attentionText(taskAttention) : null,
    showHeadLine: !taskLevel,
  };
}
