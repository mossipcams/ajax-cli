import type { ChatSessionEvent, ChatSessionReducerState } from "./model";
import { initialChatSessionReducerState } from "./model";

function bumpRevision(state: ChatSessionReducerState): ChatSessionReducerState {
  return {
    ...state,
    view: { ...state.view, revision: state.view.revision + 1 },
  };
}

export function applyStatusEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  switch (event.type) {
    case "acp_status":
      return bumpRevision({
        ...state,
        view: {
          ...state.view,
          status: {
            acpState: event.state.trim() || null,
            detail: event.detail?.trim() || null,
          },
        },
      });
    case "session_ready": {
      const base = event.reset ? initialChatSessionReducerState : state;
      if (event.busy === undefined) return event.reset ? initialChatSessionReducerState : state;
      return bumpRevision({
        ...base,
        view: {
          ...base.view,
          turn: { ...base.view.turn, busy: event.busy },
        },
      });
    }
    default:
      return state;
  }
}

export function isStatusEvent(event: ChatSessionEvent): boolean {
  return event.type === "acp_status" || event.type === "session_ready";
}
