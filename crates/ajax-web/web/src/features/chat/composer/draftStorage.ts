import type { PromptContentBlockWire } from "@/shared/lib/promptContent";
import type { ComposerState } from "./composerState";

function draftKey(handle: string): string {
  return `ajax.web.session.composer.draft.${encodeURIComponent(handle)}`;
}

function queueKey(handle: string): string {
  return `ajax.web.session.composer.queue.${encodeURIComponent(handle)}`;
}

type StoredComposerQueue = {
  text: string;
};

export function readComposerDraft(handle: string): string {
  try {
    const raw = localStorage.getItem(draftKey(handle));
    return typeof raw === "string" ? raw : "";
  } catch {
    return "";
  }
}

export function writeComposerDraft(handle: string, text: string): void {
  try {
    if (text) localStorage.setItem(draftKey(handle), text);
    else localStorage.removeItem(draftKey(handle));
  } catch {
    // Private mode / storage denied: composer still works in-memory.
  }
}

export function clearComposerDraft(handle: string): void {
  writeComposerDraft(handle, "");
}

/** Restore one editable queued follow-up; stopping is normalized to queued. */
export function readComposerQueue(handle: string): ComposerState {
  try {
    const raw = localStorage.getItem(queueKey(handle));
    if (!raw) return { status: "idle" };
    const parsed = JSON.parse(raw) as StoredComposerQueue;
    const text = typeof parsed.text === "string" ? parsed.text : "";
    if (!text.trim()) return { status: "idle" };
    return {
      status: "queued",
      text,
    };
  } catch {
    return { status: "idle" };
  }
}

export function writeComposerQueue(handle: string, state: ComposerState): void {
  try {
    if (state.status === "idle") {
      localStorage.removeItem(queueKey(handle));
      return;
    }
    const text = state.text.trim();
    const hasText = text.length > 0;
    const hasBlocks = Boolean(state.contentBlocks?.length);
    if (!hasText && !hasBlocks) {
      localStorage.removeItem(queueKey(handle));
      return;
    }
    if (hasBlocks) {
      // Attachment-bearing queues stay in memory for this tab; never persist text-only shadows.
      localStorage.removeItem(queueKey(handle));
      return;
    }
    const payload: StoredComposerQueue = { text: state.text };
    localStorage.setItem(queueKey(handle), JSON.stringify(payload));
  } catch {
    // Quota / private mode: queue still works in-memory for this tab.
  }
}

export function clearComposerQueue(handle: string): void {
  writeComposerQueue(handle, { status: "idle" });
}

/** Drop commit clears presentation-only composer state for a handle. */
export function clearComposerPresentationState(handle: string): void {
  clearComposerDraft(handle);
  clearComposerQueue(handle);
}

/** Test-only helper: detect attachment bytes that must not touch localStorage. */
export function queueHasAttachmentBytes(
  contentBlocks?: PromptContentBlockWire[],
): boolean {
  if (!contentBlocks?.length) return false;
  return contentBlocks.some(
    (block) =>
      block.type === "image" ||
      (block.type === "resource" && typeof block.blob === "string" && block.blob.length > 0),
  );
}
