// Presentation-only projection of the ACP event stream. No task truth, no
// lifecycle inference — the browser folds the wire events it is handed into the
// two shapes the session surface renders: a live head (what the agent is doing
// right now) and a settled transcript (what it has already done).
//
// Grain: the transcript is conversation, not a tool trace. Tool calls and the
// in-progress plan step live only in the head; a turn settles as one summary
// line. Pure and synchronous on purpose: every ordering rule below is a defect
// fix, and they are only cheap to prove in a reducer test.

import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
export { OPEN_FAILURE } from "@/shared/lib/webSessionTransport";

/** Map opaque ACP error strings to operator-facing copy. Human messages pass through. */
export function explainAcpError(message: string): string {
  if (/internal error/i.test(message)) {
    return "The agent rejected that request. Try sending again, or reopen the session.";
  }
  if (/ACP process exited/i.test(message)) {
    return "The agent stopped. It will restart when you reconnect.";
  }
  if (/acp request timed out/i.test(message)) {
    return "The agent did not answer in time. Try sending again.";
  }
  return message;
}

/** `prepare_task_session` refuses the upgrade when the task is not a Cursor
 * orchestration task or its worktree is gone. Both facts are already in the
 * detail payload, so no extra request is needed to say which one it was. */
export function explainOpenFailure(
  detail: { agent?: string | null; status_explanation?: string | null } | null,
): string {
  const agent = detail?.agent?.trim();
  if (agent && agent.toLowerCase() !== "cursor") {
    return `Orchestration chat needs a Cursor task — this one runs ${agent}. Open it from the task view instead.`;
  }
  const explanation = detail?.status_explanation?.trim();
  if (explanation) {
    return `Can't start the session: ${explanation}`;
  }
  return "Can't start the session. Check the task's worktree still exists.";
}

export type ToolStatus = "pending" | "in_progress" | "completed" | "failed";

export interface ToolCall {
  callId: string;
  title: string;
  kind: string;
  status: ToolStatus;
  locations: string[];
}

export interface PlanEntry {
  content: string;
  status: string;
}

export type ThreadEntry =
  | { kind: "prose"; id: string; role: "user" | "agent"; text: string }
  | { kind: "note"; id: string; tone: "info" | "error"; text: string };

export interface Decision {
  requestId: string;
  title: string;
  detail: string;
}

export interface SessionState {
  entries: ThreadEntry[];
  /** A turn is in flight: the agent owes us output. */
  busy: boolean;
  decision: Decision | null;
  /** Last agent-reported run state, shown in the head rather than appended. */
  status: string | null;
  /** Current-turn tool calls. Head only; summarized into the transcript on settle. */
  tools: ToolCall[];
  /** Current ACP plan. Head shows the in-progress step only. */
  plan: PlanEntry[];
  /** False after a tool run so the next agent chunk starts a new paragraph. */
  proseOpen: boolean;
  seq: number;
}

export type SessionAction =
  | { type: "event"; event: WebSessionServerEvent }
  | { type: "prompt"; text: string }
  | { type: "decided" }
  | { type: "reset" };

export const initialSessionState: SessionState = {
  entries: [],
  busy: false,
  decision: null,
  status: null,
  tools: [],
  plan: [],
  proseOpen: true,
  seq: 0,
};

const TOOL_STATUSES: ToolStatus[] = ["pending", "in_progress", "completed", "failed"];

function toolStatus(raw: string): ToolStatus {
  const value = raw.toLowerCase();
  return (TOOL_STATUSES as string[]).includes(value) ? (value as ToolStatus) : "in_progress";
}

/** The head shows one tool: the one still running, else the most recent. */
export function activeTool(state: SessionState): ToolCall | null {
  let last: ToolCall | null = null;
  for (const call of state.tools) {
    if (call.status === "pending" || call.status === "in_progress") last = call;
    else if (last === null || last.status === "completed" || last.status === "failed") {
      last = call;
    }
  }
  return last;
}

export function toolCallCount(state: SessionState): number {
  return state.tools.length;
}

/** The one plan line worth showing: what it is doing, not the whole checklist. */
export function activePlanStep(plan: PlanEntry[]): string | null {
  return plan.find((entry) => entry.status === "in_progress")?.content ?? null;
}

/** One line for a settled turn. Kind stays singular so we never invent plurals. */
export function summarizeTurn(tools: ToolCall[]): string | null {
  if (!tools.length) return null;
  const counts: { kind: string; n: number }[] = [];
  const indexByKind = new Map<string, number>();
  let failed = 0;
  for (const call of tools) {
    const kind = call.kind.trim() || "tool";
    const existing = indexByKind.get(kind);
    if (existing === undefined) {
      indexByKind.set(kind, counts.length);
      counts.push({ kind, n: 1 });
    } else {
      counts[existing].n += 1;
    }
    if (call.status === "failed") failed += 1;
  }
  const parts = counts.map(({ kind, n }) => `${n} ${kind}`);
  if (failed) parts.push(`${failed} failed`);
  return parts.join(" · ");
}

/** Omit over a union collapses to its shared keys, so distribute it. */
type DraftEntry = ThreadEntry extends infer T ? (T extends ThreadEntry ? Omit<T, "id"> : never) : never;

function push(state: SessionState, entry: DraftEntry): SessionState {
  const seq = state.seq + 1;
  return {
    ...state,
    seq,
    entries: [...state.entries, { ...entry, id: `e${seq}` } as ThreadEntry],
  };
}

function replaceTail(state: SessionState, entry: ThreadEntry): SessionState {
  return { ...state, entries: [...state.entries.slice(0, -1), entry] };
}

/** Streamed prose arrives one chunk at a time; consecutive chunks are one
 * paragraph, not one bubble per token. A tool run between chunks starts a new
 * paragraph so "I'll look" and "the bug is X" stay two turns of speech. */
function appendProse(
  state: SessionState,
  role: "user" | "agent",
  text: string,
): SessionState {
  const tail = state.entries[state.entries.length - 1];
  if (tail?.kind === "prose" && tail.role === role && state.proseOpen) {
    if (text === tail.text) return state;
    if (text.startsWith(tail.text) && text.length > tail.text.length) {
      return replaceTail(state, { ...tail, text });
    }
    return replaceTail(state, { ...tail, text: tail.text + text });
  }
  return { ...push(state, { kind: "prose", role, text }), proseOpen: true };
}

/** `tool_call` opens a call and `tool_call_update` revises it. Merging by
 * callId is what stops one file edit from becoming four head flashes, and
 * keeping the fields an update omits is what stops the title vanishing. */
function mergeToolCall(state: SessionState, incoming: ToolCall): SessionState {
  const index = state.tools.findIndex((call) => call.callId === incoming.callId);
  if (index < 0) {
    return { ...state, tools: [...state.tools, incoming], busy: true, proseOpen: false };
  }
  const previous = state.tools[index];
  const tools = state.tools.slice();
  tools[index] = {
    ...previous,
    ...incoming,
    title: incoming.title || previous.title,
    kind: incoming.kind || previous.kind,
    locations: incoming.locations.length ? incoming.locations : previous.locations,
  };
  return { ...state, tools, busy: true, proseOpen: false };
}

function settleTurn(state: SessionState): SessionState {
  const summary = summarizeTurn(state.tools);
  const next: SessionState = {
    ...state,
    busy: false,
    status: null,
    tools: [],
    plan: [],
    proseOpen: false,
  };
  return summary ? push(next, { kind: "note", tone: "info", text: summary }) : next;
}

export function sessionReducer(state: SessionState, action: SessionAction): SessionState {
  switch (action.type) {
    case "reset":
      return initialSessionState;

    // Optimistic operator turn: show the bubble on Enter. The host still owns
    // the durable transcript; an identical user echo is skipped in appendProse.
    case "prompt":
      return { ...appendProse(state, "user", action.text), busy: true };

    case "decided":
      return { ...state, decision: null };

    case "event":
      return applyEvent(state, action.event);
  }
}

function applyEvent(state: SessionState, event: WebSessionServerEvent): SessionState {
  switch (event.type) {
    case "message": {
      if (!event.text) return state;
      if (event.role === "thought") {
        // Reasoning is ACP telemetry. It marks the turn live and otherwise
        // stays off the surface — "Working" + the current file is enough.
        return { ...state, busy: true };
      }
      if (event.role === "agent") {
        return { ...appendProse(state, "agent", event.text), busy: true };
      }
      if (event.role === "user") return appendProse(state, "user", event.text);
      return push(state, { kind: "note", tone: "info", text: event.text });
    }

    case "prompt_accepted":
      return state;

    case "tool_call":
      return mergeToolCall(state, {
        callId: event.callId,
        title: event.title,
        kind: event.kind,
        status: toolStatus(event.status),
        locations: event.locations ?? [],
      });

    case "plan":
      return event.entries.length ? { ...state, plan: event.entries } : state;

    case "permission_request":
      // The decision belongs in the head, where the operator is already
      // looking. It is deliberately not also appended to the transcript.
      return {
        ...state,
        decision: {
          requestId: event.requestId,
          title: event.title?.trim() || "Permission required",
          detail: event.detail?.trim() || "",
        },
      };

    case "permission_resolved":
      // Replay after approve/reject must not resurrect the head prompt.
      if (state.decision?.requestId === event.requestId) {
        return { ...state, decision: null };
      }
      return state;

    case "status":
      // Run state replaces itself in the head. Appending it is what turned the
      // old thread into a column of "Status: running".
      return { ...state, status: event.state.trim() || null };

    case "turn_end": {
      const settled = settleTurn(state);
      return event.stopReason?.toLowerCase() === "error"
        ? push(settled, {
            kind: "note",
            tone: "error",
            text: "The agent stopped without a response. Check the selected model or try again.",
          })
        : settled;
    }

    case "artifact":
      // Unknown ACP update kinds are not conversation. Drop them rather than
      // pretty-printing JSON into the transcript.
      return state;

    case "error":
      return {
        ...push(
          { ...state, busy: false, status: null, tools: [], plan: [], proseOpen: false },
          { kind: "note", tone: "error", text: explainAcpError(event.message) },
        ),
      };

    case "ready":
      // The host is the authority on whether a turn is live. Replayed history
      // has no turn-start marker, so trailing events must not decide it.
      return event.busy === undefined ? state : { ...state, busy: event.busy };
  }
}
