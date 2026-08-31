import type { PromptContentBlockWire } from "@/shared/lib/promptContent";
import { hasPromptContent } from "@/shared/lib/promptContent";
import type { ComposerState } from "./composerState";
import {
  beginStopAndSend,
  clearQueue,
  composerIsStopping,
  composerQueuedContentBlocks,
  composerQueuedText,
  queueFollowUp,
  restoreQueuedDraft,
} from "./composerState";

export type ApplySubmitStateArgs = {
  connected: boolean;
  busy: boolean;
  draft: string;
  composerState: ComposerState;
  contentBlocks?: PromptContentBlockWire[];
};

export type SubmitComposerResult =
  | { action: "none" }
  | { action: "send"; text: string; clearDraft: true }
  | { action: "queue"; text: string; clearDraft: true }
  | { action: "update_queue"; text: string; clearDraft: true }
  | { action: "stop_and_send"; sendCancel: true; clearDraft: true }
  | { action: "scroll" };

/** Enter with a turn in flight queues one follow-up; Enter again stops the
 * turn and sends it. The send itself waits for the cancelled prompt to
 * resolve — see the flush effect — so the two never run together. */
export function submitComposerDraft({
  connected,
  busy,
  draft,
  composerState,
  contentBlocks = [],
}: ApplySubmitStateArgs): SubmitComposerResult {
  if (!connected) return { action: "none" };

  const text = draft.trim();
  const queued = composerQueuedText(composerState);
  const stopping = composerIsStopping(composerState);

  if (queued !== null) {
    if (hasPromptContent(text, contentBlocks)) {
      return { action: "update_queue", text, clearDraft: true };
    }
    if (busy && !stopping) return { action: "stop_and_send", sendCancel: true, clearDraft: true };
    return { action: "scroll" };
  }

  if (!hasPromptContent(text, contentBlocks)) return { action: "none" };
  if (busy) return { action: "queue", text, clearDraft: true };
  return { action: "send", text, clearDraft: true };
}

/** Pure composer-state transition for submit results already classified by
 * submitComposerDraft. Side effects (sendPrompt, sendCancel) belong in the
 * caller — see flushQueuedFollowUp for the same split. */
export function applySubmitResult(
  result: SubmitComposerResult,
  composerState: ComposerState,
  args: ApplySubmitStateArgs,
): ComposerState {
  switch (result.action) {
    case "none":
      return composerState;
    case "send":
      return composerState;
    case "queue":
      return queueFollowUp(
        composerState,
        result.text,
        args.contentBlocks?.length ? args.contentBlocks : undefined,
      );
    case "update_queue": {
      const preserved = composerQueuedContentBlocks(composerState);
      const blocks = args.contentBlocks?.length ? args.contentBlocks : preserved;
      return queueFollowUp(
        composerState,
        result.text,
        blocks?.length ? blocks : undefined,
      );
    }
    case "stop_and_send":
      if (args.busy && !composerIsStopping(composerState)) {
        return beginStopAndSend(composerState);
      }
      return composerState;
    case "scroll":
      return composerState;
  }
}

export function editQueuedFollowUp(composerState: ComposerState): {
  state: ComposerState;
  draft: string;
} | null {
  return restoreQueuedDraft(composerState);
}

export function removeQueuedFollowUp(state: ComposerState): ComposerState {
  return clearQueue(state);
}

export type FlushQueuedFollowUpIntent =
  | { type: "mark_stopped" }
  | { type: "send_prompt"; text: string; contentBlocks?: PromptContentBlockWire[] };

export type FlushQueuedFollowUpResult = {
  state: ComposerState;
  intents: FlushQueuedFollowUpIntent[];
};

/** The turn is over — either normally or because Stop & send cancelled it —
 * so the follow-up becomes the next prompt. Nothing dispatches while busy.
 * Side effects belong in intents; apply them once outside a setState updater. */
export function flushQueuedFollowUp(args: {
  composerState: ComposerState;
  busy: boolean;
  connected: boolean;
}): FlushQueuedFollowUpResult {
  const queued = composerQueuedText(args.composerState);
  if (queued === null || args.busy || !args.connected) {
    return { state: args.composerState, intents: [] };
  }

  const intents: FlushQueuedFollowUpIntent[] = [];
  if (composerIsStopping(args.composerState)) {
    intents.push({ type: "mark_stopped" });
  }
  const contentBlocks = composerQueuedContentBlocks(args.composerState);
  intents.push({
    type: "send_prompt",
    text: queued,
    ...(contentBlocks?.length ? { contentBlocks } : {}),
  });
  return { state: args.composerState, intents };
}

export function composerStateAfterFlush(
  composerState: ComposerState,
  sendSucceeded: boolean,
): ComposerState {
  return sendSucceeded ? clearQueue(composerState) : composerState;
}
