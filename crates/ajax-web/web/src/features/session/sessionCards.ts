import type {
  SessionAttentionItem,
  SessionCard,
  WebSessionMessage,
  WebSessionProgress,
} from "./types";

export const SESSION_PROGRESS_MAX_CHARS = 280;

export function truncateProgressText(
  text: string,
  max = SESSION_PROGRESS_MAX_CHARS,
): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max - 1)}…`;
}

export function currentHandleAttentions(
  handle: string,
  items: SessionAttentionItem[],
): SessionAttentionItem[] {
  return items.filter((item) => item.handle === handle);
}

export function hasCurrentHandleDecisionPending(
  handle: string,
  items: SessionAttentionItem[],
): boolean {
  return currentHandleAttentions(handle, items).length > 0;
}

export function buildSessionFeed(
  messages: WebSessionMessage[],
  handle: string,
  attentions: SessionAttentionItem[],
  progress: WebSessionProgress[] = [],
): SessionCard[] {
  const cards: SessionCard[] = messages.map((message) => {
    if (message.role === "user") {
      return {
        id: message.id,
        kind: "operator",
        text: message.text,
      };
    }
    return {
      id: message.id,
      kind: "progress",
      text: message.text,
      streaming: message.streaming,
    };
  });

  for (const item of progress) {
    if (item.kind === "tool") {
      cards.push({
        id: item.id,
        kind: "tool",
        toolName: item.toolName ?? "Tool",
        status: item.status,
        summary: item.summary,
        path: item.path,
      });
    } else {
      cards.push({
        id: item.id,
        kind: "file",
        path: item.path ?? "",
        status: item.status,
        summary: item.summary,
      });
    }
  }

  for (const attention of currentHandleAttentions(handle, attentions)) {
    cards.push({
      id: `decision:${attention.handle}:${attention.requestId}`,
      kind: "decision",
      attention,
    });
  }

  return cards;
}
