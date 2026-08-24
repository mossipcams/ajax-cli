import type { ChatSessionEvent, ChatSessionReducerState } from "./model";

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

function pushPermissionMarker(
  state: ChatSessionReducerState,
  requestId: string,
  title: string,
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
          kind: "permission",
          id: `e${seq}`,
          requestId,
          title,
          resolved: false,
        },
      ],
    },
  });
}

/** Answered locally or durably by the host: same outcome. */
export function resolvePermission(
  state: ChatSessionReducerState,
  requestId: string,
): ChatSessionReducerState {
  const permission = state.view.permission;
  const cleared: ChatSessionReducerState = bumpRevision({
    ...state,
    view: {
      ...state.view,
      permission: {
        decision:
          permission.decision?.requestId === requestId ? null : permission.decision,
        resolvedIds: permission.resolvedIds.includes(requestId)
          ? permission.resolvedIds
          : [...permission.resolvedIds, requestId],
      },
    },
  });
  const index = cleared.view.conversation.findIndex(
    (item) => item.kind === "permission" && item.requestId === requestId,
  );
  const item = index < 0 ? null : cleared.view.conversation[index];
  return item?.kind === "permission"
    ? replaceAt(cleared, index, { ...item, resolved: true })
    : cleared;
}

export function applyPermissionEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  switch (event.type) {
    case "permission_request": {
      const { requestId, detail } = event;
      // Harnesses send the title as markdown. Nothing downstream renders
      // markdown on a row, so `rm -rf …` reached the approval control with
      // literal backticks; strip them once here rather than in each reader.
      const title = event.title.replace(/`/g, "").trim();
      if (
        state.view.permission.resolvedIds.includes(requestId) ||
        state.view.permission.decision?.requestId === requestId ||
        state.view.conversation.some(
          (item) => item.kind === "permission" && item.requestId === requestId,
        )
      ) {
        return state;
      }
      return pushPermissionMarker(
        bumpRevision({
          ...state,
          view: {
            ...state.view,
            permission: {
              ...state.view.permission,
              decision: { requestId, title, detail },
            },
          },
        }),
        requestId,
        title,
      );
    }
    case "permission_resolved":
      return resolvePermission(state, event.requestId);
    default:
      return state;
  }
}

export function applyDecided(state: ChatSessionReducerState): ChatSessionReducerState {
  const decision = state.view.permission.decision;
  return decision ? resolvePermission(state, decision.requestId) : state;
}

export function isPermissionEvent(event: ChatSessionEvent): boolean {
  return event.type === "permission_request" || event.type === "permission_resolved";
}
