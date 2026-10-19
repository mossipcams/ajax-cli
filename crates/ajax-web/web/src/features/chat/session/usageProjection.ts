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
    case "usage_reset":
      return bumpRevision({
        ...state,
        view: { ...state.view, usage: { context: null } },
      });
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
    default:
      return state;
  }
}

export function isUsageEvent(event: ChatSessionEvent): boolean {
  return event.type === "context_usage" || event.type === "usage_reset";
}
