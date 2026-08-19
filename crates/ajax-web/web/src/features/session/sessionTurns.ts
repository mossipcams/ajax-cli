import type { ConversationItem } from "./sessionThread";

export interface SessionTurn {
  id: string;
  user: ConversationItem | null;
  work: ConversationItem[];
  agents: ConversationItem[];
  /** Notes and other rows outside the user/work/agent shape. */
  other: ConversationItem[];
}

function isWorkItem(item: ConversationItem): boolean {
  return (
    item.kind === "thought" ||
    item.kind === "tool" ||
    item.kind === "plan" ||
    item.kind === "permission"
  );
}

function emptyTurn(id: string): SessionTurn {
  return { id, user: null, work: [], agents: [], other: [] };
}

/** Group ACP items into operator turns: user prompt through the next user prompt. */
export function groupConversationTurns(items: ConversationItem[]): SessionTurn[] {
  const turns: SessionTurn[] = [];
  let current: SessionTurn | null = null;

  const flush = () => {
    if (!current) return;
    if (current.user || current.work.length || current.agents.length || current.other.length) {
      turns.push(current);
    }
    current = null;
  };

  for (const item of items) {
    if (item.kind === "prose" && item.role === "user") {
      flush();
      current = { ...emptyTurn(item.id), user: item };
      continue;
    }

    if (!current) {
      const orphan = emptyTurn(item.id);
      if (item.kind === "prose" && item.role === "agent") orphan.agents.push(item);
      else if (isWorkItem(item)) orphan.work.push(item);
      else orphan.other.push(item);

      const last = turns[turns.length - 1];
      if (last && !last.user) {
        last.work.push(...orphan.work);
        last.agents.push(...orphan.agents);
        last.other.push(...orphan.other);
      } else {
        turns.push(orphan);
      }
      continue;
    }

    if (item.kind === "prose" && item.role === "agent") current.agents.push(item);
    else if (isWorkItem(item)) current.work.push(item);
    else current.other.push(item);
  }

  flush();
  return turns;
}

/** Flatten a turn back into render order for legacy preamble segments. */
export function flattenTurnItems(turn: SessionTurn): ConversationItem[] {
  return [...turn.other, ...turn.work, ...turn.agents];
}
