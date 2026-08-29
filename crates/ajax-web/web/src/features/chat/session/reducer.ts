import { applyActivityEvent, isActivityEvent } from "./activityProjection";
import { applyErrorEvent, isErrorOrTurnEndEvent } from "./errorProjection";
import { applyDecided, applyPermissionEvent, isPermissionEvent } from "./permissionProjection";
import { applyElicitationAnswered, applyElicitationEvent, isElicitationEvent } from "./elicitationProjection";
import { applyStatusEvent, isStatusEvent } from "./statusProjection";
import { applyTurnEvent, applyOptimisticPrompt, isTurnEvent } from "./turnProjection";
import { applyUsageEvent, isUsageEvent } from "./usageProjection";
import type {
  ChatSessionAction,
  ChatSessionEvent,
  ChatSessionReducerState,
} from "./model";
import { initialChatSessionReducerState } from "./model";

function applyChatSessionEvent(
  state: ChatSessionReducerState,
  event: ChatSessionEvent,
): ChatSessionReducerState {
  if (isErrorOrTurnEndEvent(event)) return applyErrorEvent(state, event);
  if (isPermissionEvent(event)) return applyPermissionEvent(state, event);
  if (isElicitationEvent(event)) return applyElicitationEvent(state, event);
  if (isActivityEvent(event)) return applyActivityEvent(state, event);
  if (isUsageEvent(event)) return applyUsageEvent(state, event);
  if (isStatusEvent(event)) return applyStatusEvent(state, event);
  if (isTurnEvent(event)) return applyTurnEvent(state, event);
  return state;
}

export function reduceChatSession(
  state: ChatSessionReducerState,
  action: ChatSessionAction,
): ChatSessionReducerState {
  switch (action.type) {
    case "reset":
      return initialChatSessionReducerState;
    case "prompt":
      return applyOptimisticPrompt(state, action.text);
    case "decided":
      return applyDecided(state);
    case "elicitation_answered":
      return applyElicitationAnswered(state);
    case "event":
      return applyChatSessionEvent(state, action.event);
  }
}

export { initialChatSessionReducerState };
