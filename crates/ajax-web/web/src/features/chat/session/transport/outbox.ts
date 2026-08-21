import { MAX_FRAME_BYTES, type PendingPrompt } from "./contracts";

function promptFrame(prompt: PendingPrompt): string {
  return JSON.stringify({
    type: "prompt",
    text: prompt.text,
    clientMessageId: prompt.clientMessageId,
    ...(prompt.contentBlocks?.length ? { contentBlocks: prompt.contentBlocks } : {}),
  });
}

export function frameFits(prompt: PendingPrompt): boolean {
  return new TextEncoder().encode(promptFrame(prompt)).length <= MAX_FRAME_BYTES;
}

function outboxKey(handle: string): string {
  return `ajax.web.session.outbox.${encodeURIComponent(handle)}`;
}

function cursorKey(handle: string): string {
  return `ajax.web.session.cursor.${encodeURIComponent(handle)}`;
}

export function readOutbox(handle: string): PendingPrompt[] {
  try {
    const raw = sessionStorage.getItem(outboxKey(handle));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (item): item is PendingPrompt =>
        !!item &&
        typeof item === "object" &&
        typeof (item as PendingPrompt).text === "string" &&
        typeof (item as PendingPrompt).clientMessageId === "string",
    );
  } catch {
    return [];
  }
}

export function writeOutbox(handle: string, pending: PendingPrompt[]): void {
  try {
    if (pending.length) sessionStorage.setItem(outboxKey(handle), JSON.stringify(pending));
    else sessionStorage.removeItem(outboxKey(handle));
  } catch {
    // Private mode / storage denied: the live socket still works.
  }
}

export function readSessionCursor(handle: string): number | undefined {
  try {
    const raw = sessionStorage.getItem(cursorKey(handle));
    if (!raw) return undefined;
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : undefined;
  } catch {
    return undefined;
  }
}

export function writeSessionCursor(handle: string, cursor: number): void {
  try {
    sessionStorage.setItem(cursorKey(handle), String(cursor));
  } catch {
    // ignore storage failures
  }
}

export function clearSessionCursor(handle: string): void {
  try {
    sessionStorage.removeItem(cursorKey(handle));
  } catch {
    // ignore
  }
}

export function clearSessionOutbox(handle: string): void {
  writeOutbox(handle, []);
}

export function clearSessionTransportState(handle: string): void {
  clearSessionOutbox(handle);
  clearSessionCursor(handle);
}
