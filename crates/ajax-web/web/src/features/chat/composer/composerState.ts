/** Browser composer queue: one editable follow-up, not a second host FIFO. */
export type ComposerState =
  | { status: "idle" }
  | { status: "queued"; text: string }
  | { status: "stopping"; text: string };

export function composerQueuedText(state: ComposerState): string | null {
  if (state.status === "idle") return null;
  return state.text;
}

export function composerIsStopping(state: ComposerState): boolean {
  return state.status === "stopping";
}

export function queueFollowUp(_state: ComposerState, text: string): ComposerState {
  return { status: "queued", text };
}

export function beginStopAndSend(state: ComposerState): ComposerState {
  if (state.status === "idle") return state;
  return { status: "stopping", text: state.text };
}

export function clearQueue(_state: ComposerState = { status: "idle" }): ComposerState {
  return { status: "idle" };
}

export function restoreQueuedDraft(state: ComposerState): { state: ComposerState; draft: string } | null {
  if (state.status === "idle") return null;
  return { state: { status: "idle" }, draft: state.text };
}

/** stopping cannot exist without queued text — enforced by the union shape. */
export function assertComposerState(state: ComposerState): void {
  if (state.status === "stopping" && !state.text.trim()) {
    throw new Error("ComposerState stopping requires queued text");
  }
}
