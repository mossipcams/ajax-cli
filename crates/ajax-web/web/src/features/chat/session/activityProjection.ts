import type { ChatSessionEvent, ChatSessionReducerState, ToolCall } from "./model";

function bumpRevision(state: ChatSessionReducerState): ChatSessionReducerState {
  return {
    ...state,
    view: { ...state.view, revision: state.view.revision + 1 },
  };
}

function pushTool(state: ChatSessionReducerState, call: ToolCall): ChatSessionReducerState {
  const seq = state.seq + 1;
  return bumpRevision({
    ...state,
    seq,
    view: {
      ...state.view,
      conversation: [
        ...state.view.conversation,
        { kind: "tool", id: `e${seq}`, call },
      ],
      turn: { ...state.view.turn, busy: true, proseOpen: false },
    },
  });
}

function replaceAt(
  state: ChatSessionReducerState,
  index: number,
  item: (typeof state.view.conversation)[number],
): ChatSessionReducerState {
  const conversation = state.view.conversation.slice();
  conversation[index] = item;
  return bumpRevision({
    ...state,
    view: { ...state.view, conversation },
  });
}

/** ponytail: client clock — see sessionThread comment. */
function mergeToolCall(
  state: ChatSessionReducerState,
  incoming: Omit<ToolCall, "startedAt" | "endedAt">,
): ChatSessionReducerState {
  const now = Date.now();
  const settled = incoming.status === "completed" || incoming.status === "failed";
  const index = state.view.conversation.findIndex(
    (item) => item.kind === "tool" && item.call.callId === incoming.callId,
  );
  if (index < 0) {
    return pushTool(state, {
      ...incoming,
      startedAt: now,
      endedAt: settled ? now : undefined,
    });
  }
  const item = state.view.conversation[index];
  if (item.kind !== "tool") return state;
  const previous = item.call;
  const replaced = replaceAt(state, index, {
    ...item,
    call: {
      ...previous,
      ...incoming,
      startedAt: previous.startedAt ?? now,
      endedAt: previous.endedAt ?? (settled ? now : undefined),
      title: incoming.title || previous.title,
      kind: incoming.kind || previous.kind,
      locations: incoming.locations.length ? incoming.locations : previous.locations,
      content: incoming.content.length ? incoming.content : previous.content,
    },
  });
  return {
    ...replaced,
    view: {
      ...replaced.view,
      turn: { ...replaced.view.turn, busy: true, proseOpen: false },
    },
  };
}

export function applyActivityEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  switch (event.type) {
    case "tool_call":
      return mergeToolCall(state, event.call);
    case "plan_update": {
      if (!event.entries.length) return state;
      const index = state.view.conversation.findIndex((item) => item.kind === "plan");
      if (index < 0) {
        const seq = state.seq + 1;
        return bumpRevision({
          ...state,
          seq,
          view: {
            ...state.view,
            conversation: [
              ...state.view.conversation,
              { kind: "plan", id: `e${seq}`, entries: event.entries },
            ],
          },
        });
      }
      const item = state.view.conversation[index];
      if (item.kind !== "plan") return state;
      return replaceAt(state, index, { ...item, entries: event.entries });
    }
    default:
      return state;
  }
}

export function isActivityEvent(event: ChatSessionEvent): boolean {
  return event.type === "tool_call" || event.type === "plan_update";
}
