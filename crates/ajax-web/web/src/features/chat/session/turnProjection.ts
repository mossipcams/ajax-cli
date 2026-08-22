import type {
  ChatSessionEvent,
  ChatSessionReducerState,
  ConversationItem,
  OutputContentBlock,
} from "./model";

type DraftItem = ConversationItem extends infer T
  ? T extends ConversationItem
    ? Omit<T, "id">
    : never
  : never;

function bumpRevision(state: ChatSessionReducerState): ChatSessionReducerState {
  return {
    ...state,
    view: { ...state.view, revision: state.view.revision + 1 },
  };
}

function push(state: ChatSessionReducerState, item: DraftItem): ChatSessionReducerState {
  const seq = state.seq + 1;
  return bumpRevision({
    ...state,
    seq,
    view: {
      ...state.view,
      conversation: [
        ...state.view.conversation,
        { ...item, id: `e${seq}` } as ConversationItem,
      ],
    },
  });
}

function replaceAt(
  state: ChatSessionReducerState,
  index: number,
  item: ConversationItem,
): ChatSessionReducerState {
  const conversation = state.view.conversation.slice();
  conversation[index] = item;
  return bumpRevision({
    ...state,
    view: { ...state.view, conversation },
  });
}

function mergeContentBlocks(
  previous: OutputContentBlock[] | undefined,
  incoming: OutputContentBlock[] | undefined,
): OutputContentBlock[] | undefined {
  if (!incoming?.length) return previous;
  const merged = [...(previous ?? [])];
  for (const block of incoming) {
    if (!merged.some((existing) => JSON.stringify(existing) === JSON.stringify(block))) {
      merged.push(block);
    }
  }
  return merged.length ? merged : undefined;
}

function upsertMessage(
  state: ChatSessionReducerState,
  item:
    | {
        kind: "prose";
        role: "user" | "agent";
        text: string;
        contentBlocks?: OutputContentBlock[];
        itemId: string;
        messageId?: string;
      }
    | {
        kind: "thought";
        text: string;
        contentBlocks?: OutputContentBlock[];
        itemId: string;
        messageId?: string;
      },
): ChatSessionReducerState {
  const index = state.view.conversation.findIndex((row) => row.id === item.itemId);
  if (index >= 0) {
    const existing = state.view.conversation[index];
    if (existing.kind === "prose" && item.kind === "prose") {
      const contentBlocks = mergeContentBlocks(existing.contentBlocks, item.contentBlocks);
      if (existing.text === item.text && contentBlocks === existing.contentBlocks) return state;
      return replaceAt(state, index, {
        ...existing,
        text: item.text,
        ...(contentBlocks ? { contentBlocks } : {}),
        messageId: item.messageId ?? existing.messageId,
      });
    }
    if (existing.kind === "thought" && item.kind === "thought") {
      const contentBlocks = mergeContentBlocks(existing.contentBlocks, item.contentBlocks);
      if (existing.text === item.text && contentBlocks === existing.contentBlocks) return state;
      return replaceAt(state, index, {
        ...existing,
        text: item.text,
        ...(contentBlocks ? { contentBlocks } : {}),
        messageId: item.messageId ?? existing.messageId,
      });
    }
  }
  if (item.kind === "prose") {
    return bumpRevision({
      ...state,
      view: {
        ...state.view,
        conversation: [
          ...state.view.conversation,
          {
            kind: "prose",
            id: item.itemId,
            role: item.role,
            text: item.text,
            ...(item.contentBlocks ? { contentBlocks: item.contentBlocks } : {}),
            messageId: item.messageId,
          },
        ],
        turn: { ...state.view.turn, proseOpen: true },
      },
    });
  }
  return bumpRevision({
    ...state,
    view: {
      ...state.view,
      conversation: [
        ...state.view.conversation,
        {
          kind: "thought",
          id: item.itemId,
          text: item.text,
          ...(item.contentBlocks ? { contentBlocks: item.contentBlocks } : {}),
          messageId: item.messageId,
        },
      ],
      turn: { ...state.view.turn, proseOpen: true },
    },
  });
}

function upsertOrPushUser(
  state: ChatSessionReducerState,
  text: string,
  itemId?: string,
  messageId?: string,
  contentBlocks?: OutputContentBlock[],
): ChatSessionReducerState {
  if (itemId) {
    const tail = state.view.conversation[state.view.conversation.length - 1];
    if (
      tail?.kind === "prose" &&
      tail.role === "user" &&
      tail.text === text &&
      JSON.stringify(tail.contentBlocks ?? []) === JSON.stringify(contentBlocks ?? [])
    ) {
      return replaceAt(state, state.view.conversation.length - 1, {
        ...tail,
        id: itemId,
        messageId: messageId ?? tail.messageId,
      });
    }
    return upsertMessage(state, {
      kind: "prose",
      role: "user",
      text,
      contentBlocks,
      itemId,
      messageId,
    });
  }
  const tail = state.view.conversation[state.view.conversation.length - 1];
  if (
    tail?.kind === "prose" &&
    tail.role === "user" &&
    tail.text === text &&
    JSON.stringify(tail.contentBlocks ?? []) === JSON.stringify(contentBlocks ?? [])
  ) {
    return state;
  }
  return push(state, {
    kind: "prose",
    role: "user",
    text,
    ...(contentBlocks ? { contentBlocks } : {}),
    messageId,
  });
}

export function applyTurnEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  switch (event.type) {
    case "agent_message": {
      if (!event.text && !event.contentBlocks?.length) return state;
      if (!event.itemId) {
        const pushed = push(state, {
          kind: "prose",
          role: "agent",
          text: event.text,
          ...(event.contentBlocks ? { contentBlocks: event.contentBlocks } : {}),
          messageId: event.messageId,
        });
        return bumpRevision({
          ...pushed,
          view: {
            ...pushed.view,
            turn: { busy: true, proseOpen: true },
          },
        });
      }
      const next = upsertMessage(state, {
        kind: "prose",
        role: "agent",
        text: event.text,
        contentBlocks: event.contentBlocks,
        itemId: event.itemId,
        messageId: event.messageId,
      });
      return bumpRevision({
        ...next,
        view: { ...next.view, turn: { ...next.view.turn, busy: true } },
      });
    }
    case "thought_message": {
      if (!event.text && !event.contentBlocks?.length) return state;
      if (!event.itemId) {
        const pushed = push(state, {
          kind: "thought",
          text: event.text,
          ...(event.contentBlocks ? { contentBlocks: event.contentBlocks } : {}),
          messageId: event.messageId,
        });
        return bumpRevision({
          ...pushed,
          view: {
            ...pushed.view,
            turn: { busy: true, proseOpen: true },
          },
        });
      }
      const next = upsertMessage(state, {
        kind: "thought",
        text: event.text,
        contentBlocks: event.contentBlocks,
        itemId: event.itemId,
        messageId: event.messageId,
      });
      return bumpRevision({
        ...next,
        view: { ...next.view, turn: { ...next.view.turn, busy: true } },
      });
    }
    case "user_message":
      return upsertOrPushUser(
        state,
        event.text,
        event.itemId,
        event.messageId,
        event.contentBlocks,
      );
    case "host_note":
      return push(state, { kind: "note", tone: "info", text: event.text });
    case "system_message":
      return push(state, { kind: "note", tone: "info", text: event.text });
    case "prompt_accepted":
      return state;
    default:
      return state;
  }
}

export function applyOptimisticPrompt(
  state: ChatSessionReducerState,
  text: string,
): ChatSessionReducerState {
  const next = push(state, { kind: "prose", role: "user", text });
  return bumpRevision({
    ...next,
    view: { ...next.view, turn: { ...next.view.turn, busy: true } },
  });
}

export function settleTurn(state: ChatSessionReducerState): ChatSessionReducerState {
  return bumpRevision({
    ...state,
    view: {
      ...state.view,
      turn: { busy: false, proseOpen: false },
      status: { acpState: null, detail: null },
    },
  });
}

export function applyTurnEnd(
  state: ChatSessionReducerState,
  stopReason?: string,
): ChatSessionReducerState {
  const settled = settleTurn(state);
  if (stopReason?.toLowerCase() === "error") {
    const seq = settled.seq + 1;
    return bumpRevision({
      ...settled,
      seq,
      view: {
        ...settled.view,
        conversation: [
          ...settled.view.conversation,
          {
            kind: "note",
            id: `e${seq}`,
            tone: "error",
            text: "The agent stopped without a response. Check the selected model or try again.",
          },
        ],
      },
    });
  }
  return settled;
}

export function isTurnEvent(event: ChatSessionEvent): boolean {
  return (
    event.type === "agent_message" ||
    event.type === "thought_message" ||
    event.type === "user_message" ||
    event.type === "host_note" ||
    event.type === "system_message" ||
    event.type === "prompt_accepted"
  );
}
