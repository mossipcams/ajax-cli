import { applyActivityEvent, isActivityEvent } from "./activityProjection";
import { applyErrorEvent, isErrorOrTurnEndEvent } from "./errorProjection";
import { applyDecided, applyPermissionEvent, isPermissionEvent } from "./permissionProjection";
import { applyElicitationAnswered, applyElicitationEvent, isElicitationEvent } from "./elicitationProjection";
import { applyStatusEvent, isStatusEvent } from "./statusProjection";
import { applyTurnEvent, applyOptimisticPrompt, isTurnEvent } from "./turnProjection";
import { applyUsageEvent, isUsageEvent } from "./usageProjection";
import type {
  ConversationItem,
  ChatSessionAction,
  ChatSessionEvent,
  ChatSessionReducerState,
} from "./model";
import { initialChatSessionReducerState } from "./model";

function applyTranscriptSnapshot(
  state: ChatSessionReducerState,
  event: Extract<ChatSessionEvent, { type: "transcript_snapshot" }>,
): ChatSessionReducerState {
  const view = { ...state.view, revision: state.view.revision + 1 };
  if (event.transcriptError === undefined) {
    delete view.transcriptError;
  } else {
    view.transcriptError = event.transcriptError;
  }
  return { ...state, view };
}

function applyChatSessionEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  if (isErrorOrTurnEndEvent(event)) return applyErrorEvent(state, event);
  if (event.type === "transcript_snapshot") return applyTranscriptSnapshot(state, event);
  if (isPermissionEvent(event)) return applyPermissionEvent(state, event);
  if (isElicitationEvent(event)) return applyElicitationEvent(state, event);
  if (isActivityEvent(event)) return applyActivityEvent(state, event);
  if (isUsageEvent(event)) return applyUsageEvent(state, event);
  if (isStatusEvent(event)) return applyStatusEvent(state, event);
  if (isTurnEvent(event)) return applyTurnEvent(state, event);
  if (event.type === "history_bounds") {
    return {
      ...state,
      view: {
        ...state.view,
        serverHistory: { from: event.from, floor: event.floor },
        revision: state.view.revision + 1,
      },
    };
  }
  if (event.type === "prepend") return applyPrependOlderEvents(state, event);
  return state;
}

function offsetPrependedIds(
  items: ConversationItem[],
  taken: Set<string>,
): ConversationItem[] {
  return items.map((item) => {
    if (!taken.has(item.id)) {
      taken.add(item.id);
      return item;
    }
    let candidate = `${item.id}-h1`;
    for (let n = 1; taken.has(candidate); n += 1) {
      candidate = `${item.id}-h${n + 1}`;
    }
    taken.add(candidate);
    return { ...item, id: candidate };
  });
}

function applyPrependOlderEvents(
  state: ChatSessionReducerState,
  event: Extract<ChatSessionEvent, { type: "prepend" }>,
): ChatSessionReducerState {
  const projected = event.events.reduce(applyChatSessionEvent, initialChatSessionReducerState);
  const taken = new Set(state.view.conversation.map((item) => item.id));
  const older = offsetPrependedIds(projected.view.conversation, taken);
  return {
    ...state,
    view: {
      ...state.view,
      conversation: older.length ? [...older, ...state.view.conversation] : state.view.conversation,
      serverHistory: { from: event.from, floor: state.view.serverHistory?.floor ?? event.from },
    },
  };
}

export function reduceChatSession(
  state: ChatSessionReducerState,
  action: ChatSessionAction,
): ChatSessionReducerState {
  switch (action.type) {
    case "reset":
      return initialChatSessionReducerState;
    case "prompt":
      return applyOptimisticPrompt(state, action.text, action.clientMessageId);
    case "decided":
      return applyDecided(state);
    case "elicitation_answered":
      return applyElicitationAnswered(state);
    case "event":
      return applyChatSessionEvent(state, action.event);
    case "events":
      return action.events.reduce(applyChatSessionEvent, state);
  }
}

export type { ChatSessionReducerState };
export { initialChatSessionReducerState };
