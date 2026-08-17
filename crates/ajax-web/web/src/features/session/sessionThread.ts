// Presentation-only projection of the ACP event stream. No task truth, no
// lifecycle inference — the browser folds the wire events it is handed into a
// live head (what the agent is doing right now) and one ordered conversation.
//
// Grain: ACP separates message, thought, tool call, tool content, plan,
// permission and usage, and the conversation keeps them separated in arrival
// order. A tool call is a first-class item that revises itself in place, not a
// head flash that history forgets — the diff it wrote is the substance of the
// turn. Pure and synchronous on purpose: every ordering rule below is a defect
// fix, and they are only cheap to prove in a reducer test.

import type { ToolContent, WebSessionServerEvent } from "@/shared/lib/webSessionTransport";
export { OPEN_FAILURE } from "@/shared/lib/webSessionTransport";
export type { ToolContent } from "@/shared/lib/webSessionTransport";

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

/** `prepare_task_session` refuses the upgrade when the task cannot host an
 * orchestration session or its worktree is gone. Both facts are already in the
 * detail payload, so no extra request is needed to say which one it was. */
export function explainOpenFailure(
  detail: {
    agent?: string | null;
    status_explanation?: string | null;
    session_capable?: boolean;
  } | null,
): string {
  if (detail?.session_capable === false) {
    const agent = detail.agent?.trim();
    if (agent) {
      return `This task cannot host orchestration chat while ${agent} is running in the terminal. Open the task view instead.`;
    }
    return "This task cannot host orchestration chat. Open the task view instead.";
  }
  const explanation = detail?.status_explanation?.trim();
  if (explanation) {
    return `Can't start the session: ${explanation}`;
  }
  return "Can't start the session. Check the task still exists and its worktree is present.";
}

export type ToolStatus = "pending" | "in_progress" | "completed" | "failed";

export interface ToolCall {
  callId: string;
  title: string;
  kind: string;
  status: ToolStatus;
  locations: string[];
  /** What the call produced: printed output, a file diff. */
  content: ToolContent[];
}

export interface PlanEntry {
  content: string;
  status: string;
}

/** One row of the conversation, in arrival order. The kinds ACP distinguishes
 * stay distinct here; flattening them into prose is what made a turn unreadable
 * as anything but "something happened". */
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

/** Context window pressure, from ACP `usage_update`. */
export interface Usage {
  used: number;
  size: number;
}

export interface SessionState {
  items: ConversationItem[];
  /** A turn is in flight: the agent owes us output. */
  busy: boolean;
  decision: Decision | null;
  /** Permission ids answered locally or durably by the host. */
  resolvedPermissionIds: string[];
  /** Last agent-reported run state, shown in the head rather than appended. */
  status: string | null;
  /** Latest context pressure. One current value, not a history — a row per
   * update would bury the conversation under its own telemetry. */
  usage: Usage | null;
  /** False across a turn boundary so the next agent chunk starts a new
   * paragraph even when the previous item was also agent prose. */
  proseOpen: boolean;
  seq: number;
}

export type SessionAction =
  | { type: "event"; event: WebSessionServerEvent }
  | { type: "prompt"; text: string }
  | { type: "decided" }
  | { type: "reset" };

export const initialSessionState: SessionState = {
  items: [],
  busy: false,
  decision: null,
  resolvedPermissionIds: [],
  status: null,
  usage: null,
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
  for (const item of state.items) {
    if (item.kind !== "tool") continue;
    const call = item.call;
    if (call.status === "pending" || call.status === "in_progress") last = call;
    else if (last === null || last.status === "completed" || last.status === "failed") {
      last = call;
    }
  }
  return last;
}

export function toolCount(items: ConversationItem[]): number {
  return items.reduce((n, item) => (item.kind === "tool" ? n + 1 : n), 0);
}

/** The plan the head reads from: ACP resends the whole checklist, so the last
 * plan item is the current one. */
export function latestPlan(items: ConversationItem[]): PlanEntry[] {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    const item = items[i];
    if (item.kind === "plan") return item.entries;
  }
  return [];
}

/** The one plan line worth showing in the head: what it is doing, not the whole
 * checklist — the checklist itself is in the conversation. */
export function activePlanStep(plan: PlanEntry[]): string | null {
  return plan.find((entry) => entry.status === "in_progress")?.content ?? null;
}

/** Omit over a union collapses to its shared keys, so distribute it. */
type DraftItem = ConversationItem extends infer T
  ? T extends ConversationItem
    ? Omit<T, "id">
    : never
  : never;

function push(state: SessionState, item: DraftItem): SessionState {
  const seq = state.seq + 1;
  return {
    ...state,
    seq,
    items: [...state.items, { ...item, id: `e${seq}` } as ConversationItem],
  };
}

function replaceAt(state: SessionState, index: number, item: ConversationItem): SessionState {
  const items = state.items.slice();
  items[index] = item;
  return { ...state, items };
}

/** Streamed items from the host arrive as full-content updates keyed by itemId. */
function upsertMessage(
  state: SessionState,
  item:
    | { kind: "prose"; role: "user" | "agent"; text: string; itemId: string; messageId?: string }
    | { kind: "thought"; text: string; itemId: string; messageId?: string },
): SessionState {
  const index = state.items.findIndex((row) => row.id === item.itemId);
  if (index >= 0) {
    const existing = state.items[index];
    if (existing.kind === "prose" && item.kind === "prose") {
      if (existing.text === item.text) return state;
      return replaceAt(state, index, {
        ...existing,
        text: item.text,
        messageId: item.messageId ?? existing.messageId,
      });
    }
    if (existing.kind === "thought" && item.kind === "thought") {
      if (existing.text === item.text) return state;
      return replaceAt(state, index, {
        ...existing,
        text: item.text,
        messageId: item.messageId ?? existing.messageId,
      });
    }
  }
  if (item.kind === "prose") {
    return {
      ...state,
      items: [
        ...state.items,
        {
          kind: "prose",
          id: item.itemId,
          role: item.role,
          text: item.text,
          messageId: item.messageId,
        },
      ],
      proseOpen: true,
    };
  }
  return {
    ...state,
    items: [
      ...state.items,
      {
        kind: "thought",
        id: item.itemId,
        text: item.text,
        messageId: item.messageId,
      },
    ],
    proseOpen: true,
  };
}

function upsertOrPushUser(state: SessionState, text: string, itemId?: string, messageId?: string) {
  if (itemId) {
    const tail = state.items[state.items.length - 1];
    if (tail?.kind === "prose" && tail.role === "user" && tail.text === text) {
      return replaceAt(state, state.items.length - 1, {
        ...tail,
        id: itemId,
        messageId: messageId ?? tail.messageId,
      });
    }
    return upsertMessage(state, { kind: "prose", role: "user", text, itemId, messageId });
  }
  const tail = state.items[state.items.length - 1];
  if (tail?.kind === "prose" && tail.role === "user" && tail.text === text) return state;
  return push(state, { kind: "prose", role: "user", text, messageId });
}

/** `tool_call` opens a call and `tool_call_update` revises it. Merging by
 * callId is what stops one file edit from becoming four rows, and keeping the
 * fields an update omits is what stops the title vanishing.
 *
 * The call keeps its original position: a completing edit belongs where the
 * agent made it, not at the bottom of the conversation. */
function mergeToolCall(state: SessionState, incoming: ToolCall): SessionState {
  const index = state.items.findIndex(
    (item) => item.kind === "tool" && item.call.callId === incoming.callId,
  );
  if (index < 0) {
    return { ...push(state, { kind: "tool", call: incoming }), busy: true, proseOpen: false };
  }
  const item = state.items[index];
  if (item.kind !== "tool") return state;
  const previous = item.call;
  return {
    ...replaceAt(state, index, {
      ...item,
      call: {
        ...previous,
        ...incoming,
        title: incoming.title || previous.title,
        kind: incoming.kind || previous.kind,
        locations: incoming.locations.length ? incoming.locations : previous.locations,
        // An update omitting `content` is not an update clearing it; ACP sends
        // the whole array when it has one to send.
        content: incoming.content.length ? incoming.content : previous.content,
      },
    }),
    busy: true,
    proseOpen: false,
  };
}

function settleTurn(state: SessionState): SessionState {
  return { ...state, busy: false, status: null, proseOpen: false };
}

/** Answered here or answered durably by the host: same outcome. Clearing the
 * head prompt and marking the marker row resolved must stay together, or a
 * local approve leaves the conversation claiming the ask is still open. */
function resolvePermission(state: SessionState, requestId: string): SessionState {
  const cleared: SessionState = {
    ...state,
    decision: state.decision?.requestId === requestId ? null : state.decision,
    resolvedPermissionIds: state.resolvedPermissionIds.includes(requestId)
      ? state.resolvedPermissionIds
      : [...state.resolvedPermissionIds, requestId],
  };
  const index = cleared.items.findIndex(
    (item) => item.kind === "permission" && item.requestId === requestId,
  );
  const item = index < 0 ? null : cleared.items[index];
  return item?.kind === "permission"
    ? replaceAt(cleared, index, { ...item, resolved: true })
    : cleared;
}

export function sessionReducer(state: SessionState, action: SessionAction): SessionState {
  switch (action.type) {
    case "reset":
      return initialSessionState;

    // Optimistic operator turn: show the bubble on Enter. The host still owns
    // the durable transcript; an identical user echo is skipped downstream.
    case "prompt":
      return {
        ...push(state, { kind: "prose", role: "user", text: action.text }),
        busy: true,
      };

    case "decided":
      return state.decision ? resolvePermission(state, state.decision.requestId) : state;

    case "event":
      return applyEvent(state, action.event);
  }
}

function applyEvent(state: SessionState, event: WebSessionServerEvent): SessionState {
  switch (event.type) {
    case "message": {
      if (!event.text) return state;
      if (event.role === "thought") {
        if (!event.itemId) {
          return {
            ...push(state, {
              kind: "thought",
              text: event.text,
              messageId: event.messageId,
            }),
            busy: true,
            proseOpen: true,
          };
        }
        return {
          ...upsertMessage(state, {
            kind: "thought",
            text: event.text,
            itemId: event.itemId,
            messageId: event.messageId,
          }),
          busy: true,
        };
      }
      if (event.role === "agent") {
        if (!event.itemId) {
          return {
            ...push(state, {
              kind: "prose",
              role: "agent",
              text: event.text,
              messageId: event.messageId,
            }),
            busy: true,
            proseOpen: true,
          };
        }
        return {
          ...upsertMessage(state, {
            kind: "prose",
            role: "agent",
            text: event.text,
            itemId: event.itemId,
            messageId: event.messageId,
          }),
          busy: true,
        };
      }
      if (event.role === "user") {
        return upsertOrPushUser(state, event.text, event.itemId, event.messageId);
      }
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
        content: event.content ?? [],
      });

    case "plan": {
      // ACP resends the whole checklist on every revision, so the plan revises
      // itself in place rather than stacking a copy per update.
      if (!event.entries.length) return state;
      const index = state.items.findIndex((item) => item.kind === "plan");
      if (index < 0) return push(state, { kind: "plan", entries: event.entries });
      const item = state.items[index];
      if (item.kind !== "plan") return state;
      return replaceAt(state, index, { ...item, entries: event.entries });
    }

    case "usage":
      return event.size > 0 ? { ...state, usage: { used: event.used, size: event.size } } : state;

    case "permission_request":
      // The buttons live in the head — sticky, so on a phone the ask cannot
      // scroll away. The conversation gets a marker row in its place so the
      // history still reads as "it asked here", not as a silent gap.
      if (
        state.resolvedPermissionIds.includes(event.requestId) ||
        state.decision?.requestId === event.requestId ||
        state.items.some(
          (item) => item.kind === "permission" && item.requestId === event.requestId,
        )
      ) {
        return state;
      }
      return push(
        {
          ...state,
          decision: {
            requestId: event.requestId,
            title: event.title?.trim() || "Permission required",
            detail: event.detail?.trim() || "",
          },
        },
        {
          kind: "permission",
          requestId: event.requestId,
          title: event.title?.trim() || "Permission required",
          resolved: false,
        },
      );

    case "permission_resolved":
      // Replay after approve/reject must not resurrect the head prompt.
      return resolvePermission(state, event.requestId);

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
      return push(settleTurn(state), {
        kind: "note",
        tone: "error",
        text: explainAcpError(event.message),
      });

    case "ready":
      // The host is the authority on whether a turn is live. Replayed history
      // has no turn-start marker, so trailing events must not decide it.
      {
        const base = event.reset ? initialSessionState : state;
        return event.busy === undefined ? base : { ...base, busy: event.busy };
      }
  }
}
