import type { ChatSessionEvent, ChatSessionReducerState } from "./model";

function bumpRevision(state: ChatSessionReducerState): ChatSessionReducerState {
  return {
    ...state,
    view: { ...state.view, revision: state.view.revision + 1 },
  };
}

export function applyUsageEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  switch (event.type) {
    case "context_usage":
      if (event.size <= 0) return state;
      return bumpRevision({
        ...state,
        view: {
          ...state.view,
          usage: {
            ...state.view.usage,
            context: { used: event.used, size: event.size },
          },
        },
      });
    case "turn_usage":
      return bumpRevision({
        ...state,
        view: {
          ...state.view,
          usage: {
            ...state.view.usage,
            turn: event.usage,
          },
        },
      });
    default:
      return state;
  }
}

export function isUsageEvent(event: ChatSessionEvent): boolean {
  return event.type === "context_usage" || event.type === "turn_usage";
}
