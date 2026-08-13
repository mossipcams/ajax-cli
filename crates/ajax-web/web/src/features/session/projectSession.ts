// Pure fold of the ACP event log into session presentation grain. No React, no
// task truth — the browser projects wire events into live-head and transcript shapes.

/** Emitted when the upgrade is refused. The browser cannot expose the HTTP
 * status or body of a failed WebSocket handshake, so this string carries no
 * reason — callers recover one from task truth. */
export const OPEN_FAILURE = "Session WebSocket failed to open";

export type ServerEvent =
  | { type: "ready"; model?: string }
  | { type: "message"; role: string; text: string }
  | { type: "artifact"; kind: string; title?: string | null; body?: string | null }
  | {
      type: "tool_call";
      callId: string;
      title: string;
      kind: string;
      status: string;
      locations?: string[];
    }
  | { type: "plan"; entries: { content: string; status: string }[] }
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

export interface SessionView {
  entries: ThreadEntry[];
  busy: boolean;
  decision: Decision | null;
  status: string | null;
  tools: ToolCall[];
  plan: PlanEntry[];
}

interface FoldState {
  entries: ThreadEntry[];
  busy: boolean;
  decision: Decision | null;
  status: string | null;
  tools: ToolCall[];
  plan: PlanEntry[];
  proseOpen: boolean;
  seq: number;
}

const TOOL_STATUSES: ToolStatus[] = ["pending", "in_progress", "completed", "failed"];

function toolStatus(raw: string): ToolStatus {
  const value = raw.toLowerCase();
  return (TOOL_STATUSES as string[]).includes(value) ? (value as ToolStatus) : "in_progress";
}

export function activeTool(view: Pick<SessionView, "tools">): ToolCall | null {
  let last: ToolCall | null = null;
  for (const call of view.tools) {
    if (call.status === "pending" || call.status === "in_progress") last = call;
    else if (last === null || last.status === "completed" || last.status === "failed") {
      last = call;
    }
  }
  return last;
}

export function activePlanStep(plan: PlanEntry[]): string | null {
  return plan.find((entry) => entry.status === "in_progress")?.content ?? null;
}

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

type DraftEntry = ThreadEntry extends infer T ? (T extends ThreadEntry ? Omit<T, "id"> : never) : never;

function push(state: FoldState, entry: DraftEntry): FoldState {
  const seq = state.seq + 1;
  return {
    ...state,
    seq,
    entries: [...state.entries, { ...entry, id: `e${seq}` } as ThreadEntry],
  };
}

function replaceTail(state: FoldState, entry: ThreadEntry): FoldState {
  return { ...state, entries: [...state.entries.slice(0, -1), entry] };
}

function looksLikeNewUtterance(tail: string, incoming: string): boolean {
  if (incoming !== incoming.trimStart()) return false;
  if (!/^[A-Z]/.test(incoming)) return false;
  return /(?:\n```\s*$)|(?:[.!?]["']?\s*$)/.test(tail);
}

function appendProse(state: FoldState, role: "user" | "agent", text: string): FoldState {
  const tail = state.entries[state.entries.length - 1];
  if (tail?.kind === "prose" && tail.role === role && state.proseOpen) {
    if (text === tail.text) return state;
    if (text.startsWith(tail.text) && text.length > tail.text.length) {
      return replaceTail(state, { ...tail, text });
    }
    if (looksLikeNewUtterance(tail.text, text)) {
      return { ...push(state, { kind: "prose", role, text }), proseOpen: true };
    }
    return replaceTail(state, { ...tail, text: tail.text + text });
  }
  return { ...push(state, { kind: "prose", role, text }), proseOpen: true };
}

function mergeToolCall(state: FoldState, incoming: ToolCall): FoldState {
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

function settleTurn(state: FoldState): FoldState {
  const summary = summarizeTurn(state.tools);
  const next: FoldState = {
    ...state,
    busy: false,
    status: null,
    tools: [],
    plan: [],
    proseOpen: false,
  };
  return summary ? push(next, { kind: "note", tone: "info", text: summary }) : next;
}

function applyEvent(state: FoldState, event: ServerEvent): FoldState {
  switch (event.type) {
    case "message": {
      if (!event.text) return state;
      if (event.role === "thought") {
        return { ...state, busy: true };
      }
      if (event.role === "agent") {
        return { ...appendProse(state, "agent", event.text), busy: true };
      }
      if (event.role === "user") return appendProse(state, "user", event.text);
      return push(state, { kind: "note", tone: "info", text: event.text });
    }
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
      return {
        ...state,
        decision: {
          requestId: event.requestId,
          title: event.title?.trim() || "Permission required",
          detail: event.detail?.trim() || "",
        },
      };
    case "permission_resolved":
      if (state.decision?.requestId === event.requestId) {
        return { ...state, decision: null };
      }
      return state;
    case "status":
      return { ...state, status: event.state.trim() || null };
    case "turn_end":
      return settleTurn(state);
    case "artifact":
      return state;
    case "error":
      return {
        ...push(
          { ...state, busy: false, status: null, tools: [], plan: [], proseOpen: false },
          { kind: "note", tone: "error", text: explainAcpError(event.message) },
        ),
      };
    case "ready":
      return state;
  }
}

const initialFold: FoldState = {
  entries: [],
  busy: false,
  decision: null,
  status: null,
  tools: [],
  plan: [],
  proseOpen: true,
  seq: 0,
};

export function projectSession(events: ServerEvent[]): SessionView {
  const folded = events.reduce(applyEvent, initialFold);
  return {
    entries: folded.entries,
    busy: folded.busy,
    decision: folded.decision,
    status: folded.status,
    tools: folded.tools,
    plan: folded.plan,
  };
}
