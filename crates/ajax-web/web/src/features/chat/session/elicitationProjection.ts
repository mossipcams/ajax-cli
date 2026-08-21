import type { ChatSessionEvent, ChatSessionReducerState } from "./model";
import { parseElicitationFormSchema } from "@/shared/lib/liveSessionElicitation";

function bumpRevision(state: ChatSessionReducerState): ChatSessionReducerState {
  return {
    ...state,
    view: { ...state.view, revision: state.view.revision + 1 },
  };
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

function pushElicitationMarker(
  state: ChatSessionReducerState,
  requestId: string,
  message: string,
): ChatSessionReducerState {
  const seq = state.seq + 1;
  return bumpRevision({
    ...state,
    seq,
    view: {
      ...state.view,
      conversation: [
        ...state.view.conversation,
        {
          kind: "elicitation",
          id: `e${seq}`,
          requestId,
          message,
          resolved: false,
        },
      ],
    },
  });
}

export function resolveElicitation(
  state: ChatSessionReducerState,
  requestId: string,
): ChatSessionReducerState {
  const elicitation = state.view.elicitation;
  const cleared: ChatSessionReducerState = bumpRevision({
    ...state,
    view: {
      ...state.view,
      elicitation: {
        decision:
          elicitation.decision?.requestId === requestId ? null : elicitation.decision,
        resolvedIds: elicitation.resolvedIds.includes(requestId)
          ? elicitation.resolvedIds
          : [...elicitation.resolvedIds, requestId],
      },
    },
  });
  const index = cleared.view.conversation.findIndex(
    (item) => item.kind === "elicitation" && item.requestId === requestId,
  );
  const item = index < 0 ? null : cleared.view.conversation[index];
  return item?.kind === "elicitation"
    ? replaceAt(cleared, index, { ...item, resolved: true })
    : cleared;
}

export function applyElicitationEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  switch (event.type) {
    case "elicitation_request": {
      const { requestId, message, schema } = event;
      if (
        state.view.elicitation.resolvedIds.includes(requestId) ||
        state.view.elicitation.decision?.requestId === requestId ||
        state.view.conversation.some(
          (item) => item.kind === "elicitation" && item.requestId === requestId,
        )
      ) {
        return state;
      }
      const fields = parseElicitationFormSchema(schema);
      return pushElicitationMarker(
        bumpRevision({
          ...state,
          view: {
            ...state.view,
            elicitation: {
              ...state.view.elicitation,
              decision: { requestId, message, schema, fields },
            },
          },
        }),
        requestId,
        message,
      );
    }
    case "elicitation_resolved":
      return resolveElicitation(state, event.requestId);
    default:
      return state;
  }
}

export function applyElicitationAnswered(state: ChatSessionReducerState): ChatSessionReducerState {
  const decision = state.view.elicitation.decision;
  return decision ? resolveElicitation(state, decision.requestId) : state;
}

export function isElicitationEvent(event: ChatSessionEvent): boolean {
  return event.type === "elicitation_request" || event.type === "elicitation_resolved";
}
