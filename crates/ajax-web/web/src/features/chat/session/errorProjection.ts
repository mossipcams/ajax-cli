import { explainAcpError } from "./errors";
import type { ChatSessionEvent, ChatSessionReducerState } from "./model";
import { applyTurnEnd, settleTurn } from "./turnProjection";

function bumpRevision(state: ChatSessionReducerState): ChatSessionReducerState {
  return {
    ...state,
    view: { ...state.view, revision: state.view.revision + 1 },
  };
}

function pushNote(
  state: ChatSessionReducerState,
  tone: "info" | "error",
  text: string,
): ChatSessionReducerState {
  const seq = state.seq + 1;
  return bumpRevision({
    ...state,
    seq,
    view: {
      ...state.view,
      conversation: [
        ...state.view.conversation,
        { kind: "note", id: `e${seq}`, tone, text },
      ],
    },
  });
}

export function applyErrorEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  switch (event.type) {
    case "session_error":
      return pushNote(settleTurn(state), "error", explainAcpError(event.message));
    case "turn_end":
      return applyTurnEnd(state, event.stopReason);
    default:
      return state;
  }
}

export function isErrorOrTurnEndEvent(event: ChatSessionEvent): boolean {
  return event.type === "session_error" || event.type === "turn_end";
}
