import type { ComposerState } from "./composerState";
import {
  beginStopAndSend,
  clearQueue,
  composerIsStopping,
  composerQueuedText,
  queueFollowUp,
  restoreQueuedDraft,
} from "./composerState";

export type SubmitComposerArgs = {
  connected: boolean;
  busy: boolean;
  draft: string;
  composerState: ComposerState;
  sendPrompt: (text: string) => boolean;
  sendCancel: () => void;
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
}: Omit<SubmitComposerArgs, "sendPrompt" | "sendCancel">): SubmitComposerResult {
  if (!connected) return { action: "none" };

  const text = draft.trim();
  const queued = composerQueuedText(composerState);
  const stopping = composerIsStopping(composerState);

  if (queued !== null) {
    if (text) return { action: "update_queue", text, clearDraft: true };
    if (busy && !stopping) return { action: "stop_and_send", sendCancel: true, clearDraft: true };
    return { action: "scroll" };
  }

  if (!text) return { action: "none" };
  if (busy) return { action: "queue", text, clearDraft: true };
  return { action: "send", text, clearDraft: true };
}

export function applySubmitResult(
  result: SubmitComposerResult,
  composerState: ComposerState,
  args: SubmitComposerArgs,
): ComposerState {
  switch (result.action) {
    case "none":
      return composerState;
    case "send":
      if (args.sendPrompt(result.text)) return clearQueue(composerState);
      return composerState;
    case "queue":
      return queueFollowUp(composerState, result.text);
    case "update_queue":
      return queueFollowUp(composerState, result.text);
    case "stop_and_send":
      if (args.busy && !composerIsStopping(composerState)) {
        args.sendCancel();
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

/** The turn is over — either normally or because Stop & send cancelled it —
 * so the follow-up becomes the next prompt. Nothing dispatches while busy. */
export function flushQueuedFollowUp(args: {
  composerState: ComposerState;
  busy: boolean;
  connected: boolean;
  sendPrompt: (text: string) => boolean;
  markStopped: () => void;
}): ComposerState {
  const queued = composerQueuedText(args.composerState);
  if (queued === null || args.busy || !args.connected) return args.composerState;

  if (composerIsStopping(args.composerState)) {
    args.markStopped();
  }
  if (args.sendPrompt(queued)) return clearQueue(args.composerState);
  return args.composerState;
}
