// Presentation-only projection of the ACP event stream. No task truth, no
// lifecycle inference — the browser folds the wire events it is handed into the
// two shapes the session surface renders: a live head (what the agent is doing
// right now) and a settled transcript (what it has already done).
//
// Pure and synchronous on purpose: every ordering rule below is a defect fix,
// and they are only cheap to prove in a reducer test.

import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
export { OPEN_FAILURE } from "@/shared/lib/webSessionTransport";

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
  | { kind: "tools"; id: string; calls: ToolCall[] }
  | { kind: "plan"; id: string; entries: PlanEntry[] }
  | { kind: "note"; id: string; tone: "info" | "error"; text: string; body?: string };

export interface Decision {
  requestId: string;
  title: string;
  detail: string;
}

export interface SessionState {
  entries: ThreadEntry[];
  /** A turn is in flight: the agent owes us output. */
  busy: boolean;
  /** Live reasoning. Ephemeral — it never lands in the transcript. */
  thought: string | null;
  decision: Decision | null;
  /** Last agent-reported run state, shown in the head rather than appended. */
  status: string | null;
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
  thought: null,
  decision: null,
  status: null,
  seq: 0,
};

/** Sized to fit the head's two-line clamp at phone width. Longer than this and
 * the clamp hides the newest words again — which is the defect this exists to
 * prevent, so the bound and the clamp must stay in agreement. */
const THOUGHT_TAIL_CHARS = 100;

/** The head answers "what is the agent doing *now*", so reasoning keeps its
 * tail. Accumulating the whole block and clamping to two lines in CSS showed
 * the opening words frozen for the rest of the turn — and grew without bound. */
export function thoughtTail(text: string): string {
  const collapsed = text.replace(/\s+/g, " ");
  if (collapsed.length <= THOUGHT_TAIL_CHARS) return collapsed;
  const tail = collapsed.slice(-THOUGHT_TAIL_CHARS);
  const boundary = tail.indexOf(" ");
  return `…${boundary === -1 ? tail : tail.slice(boundary + 1)}`;
}

const TOOL_STATUSES: ToolStatus[] = ["pending", "in_progress", "completed", "failed"];

function toolStatus(raw: string): ToolStatus {
  const value = raw.toLowerCase();
  return (TOOL_STATUSES as string[]).includes(value) ? (value as ToolStatus) : "in_progress";
}

/** The head shows one tool: the one still running, else the most recent. */
export function activeTool(state: SessionState): ToolCall | null {
  let last: ToolCall | null = null;
  for (const entry of state.entries) {
    if (entry.kind !== "tools") continue;
    for (const call of entry.calls) {
      if (call.status === "pending" || call.status === "in_progress") last = call;
      else if (last === null || last.status === "completed" || last.status === "failed") {
        last = call;
      }
    }
  }
  return last;
}

export function toolCallCount(state: SessionState): number {
  return state.entries.reduce(
    (total, entry) => (entry.kind === "tools" ? total + entry.calls.length : total),
    0,
  );
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
 * paragraph, not one bubble per token. */
function appendProse(
  state: SessionState,
  role: "user" | "agent",
  text: string,
): SessionState {
  const tail = state.entries[state.entries.length - 1];
  if (tail?.kind === "prose" && tail.role === role) {
    return replaceTail(state, { ...tail, text: tail.text + text });
  }
  return push(state, { kind: "prose", role, text });
}

/** `tool_call` opens a call and `tool_call_update` revises it. Merging by
 * callId is what stops one file edit from becoming four transcript rows, and
 * keeping the fields an update omits is what stops the title vanishing. */
function mergeToolCall(state: SessionState, incoming: ToolCall): SessionState {
  const tail = state.entries[state.entries.length - 1];
  if (tail?.kind === "tools") {
    const index = tail.calls.findIndex((call) => call.callId === incoming.callId);
    if (index >= 0) {
      const previous = tail.calls[index];
      const calls = tail.calls.slice();
      calls[index] = {
        ...previous,
        ...incoming,
        title: incoming.title || previous.title,
        kind: incoming.kind || previous.kind,
        locations: incoming.locations.length ? incoming.locations : previous.locations,
      };
      return replaceTail(state, { ...tail, calls });
    }
    return replaceTail(state, { ...tail, calls: [...tail.calls, incoming] });
  }

  // An update can arrive after prose split the run; revise the call in place
  // wherever it already lives rather than opening a duplicate.
  for (let i = state.entries.length - 1; i >= 0; i -= 1) {
    const entry = state.entries[i];
    if (entry.kind !== "tools") continue;
    const index = entry.calls.findIndex((call) => call.callId === incoming.callId);
    if (index < 0) continue;
    const previous = entry.calls[index];
    const calls = entry.calls.slice();
    calls[index] = {
      ...previous,
      ...incoming,
      title: incoming.title || previous.title,
      kind: incoming.kind || previous.kind,
      locations: incoming.locations.length ? incoming.locations : previous.locations,
    };
    const entries = state.entries.slice();
    entries[i] = { ...entry, calls };
    return { ...state, entries };
  }

  return push(state, { kind: "tools", calls: [incoming] });
}

/** ACP plans are full replacements, so revising one in place is what keeps a
 * long turn from stacking eight near-identical plan cards. */
function upsertPlan(state: SessionState, entries: PlanEntry[]): SessionState {
  if (!entries.length) return state;
  const index = state.entries.findIndex((entry) => entry.kind === "plan");
  if (index < 0) return push(state, { kind: "plan", entries });
  const next = state.entries.slice();
  next[index] = { ...(next[index] as Extract<ThreadEntry, { kind: "plan" }>), entries };
  return { ...state, entries: next };
}

export function sessionReducer(state: SessionState, action: SessionAction): SessionState {
  switch (action.type) {
    case "reset":
      return initialSessionState;

    case "prompt":
      return { ...appendProse(state, "user", action.text), busy: true, thought: null };

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
        return {
          ...state,
          busy: true,
          thought: thoughtTail((state.thought ?? "") + event.text),
        };
      }
      if (event.role === "agent") {
        return { ...appendProse(state, "agent", event.text), busy: true, thought: null };
      }
      if (event.role === "user") return appendProse(state, "user", event.text);
      return push(state, { kind: "note", tone: "info", text: event.text });
    }

    case "tool_call":
      return {
        ...mergeToolCall(state, {
          callId: event.callId,
          title: event.title,
          kind: event.kind,
          status: toolStatus(event.status),
          locations: event.locations ?? [],
        }),
        busy: true,
        thought: null,
      };

    case "plan":
      return upsertPlan(state, event.entries);

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

    case "status":
      // Run state replaces itself in the head. Appending it is what turned the
      // old thread into a column of "Status: running".
      return { ...state, status: event.state.trim() || null };

    case "turn_end":
      return { ...state, busy: false, thought: null, status: null };

    case "artifact": {
      const title = event.title?.trim();
      const body = event.body?.trim();
      if (!title && !body) return state;
      return push(state, {
        kind: "note",
        tone: "info",
        text: title || event.kind || "Update",
        body: body || undefined,
      });
    }

    case "error":
      return {
        ...push(state, { kind: "note", tone: "error", text: event.message }),
        busy: false,
        thought: null,
      };

    case "ready":
      return state;
  }
}
