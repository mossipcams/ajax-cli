import type { PromptContentBlockWire } from "@/shared/lib/promptContent";

/** Browser composer queue: one editable follow-up mirrored on the host FIFO. */
export type ComposerState =
  | { status: "idle" }
  | {
      status: "queued";
      text: string;
      contentBlocks?: PromptContentBlockWire[];
      clientMessageId?: string;
    }
  | {
      status: "stopping";
      text: string;
      contentBlocks?: PromptContentBlockWire[];
      clientMessageId?: string;
    };

export function composerQueuedText(state: ComposerState): string | null {
  if (state.status === "idle") return null;
  return state.text;
}

export function composerQueuedContentBlocks(
  state: ComposerState,
): PromptContentBlockWire[] | undefined {
  if (state.status === "idle") return undefined;
  return state.contentBlocks;
}

export function composerIsStopping(state: ComposerState): boolean {
  return state.status === "stopping";
}

export function queueFollowUp(
  _state: ComposerState,
  text: string,
  contentBlocks?: PromptContentBlockWire[],
  clientMessageId?: string,
): ComposerState {
  return {
    status: "queued",
    text,
    ...(contentBlocks?.length ? { contentBlocks } : {}),
    ...(clientMessageId ? { clientMessageId } : {}),
  };
}

export function beginStopAndSend(state: ComposerState): ComposerState {
  if (state.status === "idle") return state;
  return {
    status: "stopping",
    text: state.text,
    ...(state.contentBlocks?.length ? { contentBlocks: state.contentBlocks } : {}),
    ...(state.clientMessageId ? { clientMessageId: state.clientMessageId } : {}),
  };
}

export function clearQueue(_state: ComposerState = { status: "idle" }): ComposerState {
  return { status: "idle" };
}

export function restoreQueuedDraft(state: ComposerState): {
  state: ComposerState;
  draft: string;
  contentBlocks?: PromptContentBlockWire[];
} | null {
  if (state.status === "idle") return null;
  return {
    state: { status: "idle" },
    draft: state.text,
    ...(state.contentBlocks?.length ? { contentBlocks: state.contentBlocks } : {}),
  };
}

/** stopping cannot exist without queued text or attachments — enforced by the union shape. */
export function assertComposerState(state: ComposerState): void {
  if (
    state.status === "stopping" &&
    !state.text.trim() &&
    !state.contentBlocks?.length
  ) {
    throw new Error("ComposerState stopping requires queued text or attachments");
  }
}
