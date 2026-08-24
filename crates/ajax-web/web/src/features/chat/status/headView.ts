import type { ChatSessionView, Decision, ElicitationDecision } from "../session/public";

export type HeadState = "decision" | "working" | "attention" | "idle";

/** Narrow task-attention input — no BrowserTaskDetail in status presentation. */
export interface ChatTaskAttention {
  status: "waiting" | "error";
  explanation?: string | null;
}

export interface ChatHeadView {
  state: HeadState;
  tone: string;
  connected: boolean;
  activityAgeMs: number;
  decision: Decision | null;
  /** Whether the turn has produced anything the transcript can narrate yet. */
  hasActivity: boolean;
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

function agentNeedsYou(status: string | null): boolean {
  const token = status?.trim().toLowerCase();
  return token === "waiting" || token === "requires_action";
}

function agentWorking(status: string | null): boolean {
  return status?.trim().toLowerCase() === "running";
}

export function headState(
  decision: Decision | null,
  elicitation: ElicitationDecision | null,
  busy: boolean,
  taskAttention: ChatTaskAttention | null,
  agentStatus: string | null,
): HeadState {
  if (decision || elicitation) return "decision";
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
  hasActivity: boolean;
  activityAgeMs: number;
  connected: boolean;
}): ChatHeadView {
  const { session, taskAttention, hasActivity, activityAgeMs, connected } = input;
  const decision = session.permission.decision;
  const elicitation = session.elicitation.decision;
  const state = headState(
    decision,
    elicitation,
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
    hasActivity,
    usage: session.usage.context,
    turnUsage: session.usage.turn,
    taskAttention: taskLevel ? taskAttention : null,
    attentionText: taskLevel && taskAttention ? attentionText(taskAttention) : null,
    // Task attention replaces the head line, but `Reconnecting` has nowhere
    // else to live — and a task waiting for review is the state most sessions
    // rest in, so a dropped socket was invisible in the common case.
    showHeadLine: !taskLevel || !connected,
  };
}
