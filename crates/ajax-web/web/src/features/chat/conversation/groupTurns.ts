import type { ConversationItem } from "../session/public";

export interface ConversationTurn {
  id: string;
  user: ConversationItem | null;
  /** Everything behind the turn's one activity disclosure. */
  work: ConversationItem[];
  agents: ConversationItem[];
  /** Rows that stay in the transcript: an ask the operator still owes an answer
   * to, an error, a system divider. */
  other: ConversationItem[];
}

/** Protocol detail belongs behind the disclosure; only what the operator must
 * act on stays in the conversation. An unanswered permission or elicitation is
 * an action, so it stays out here until it is resolved and becomes history. */
function isWorkItem(item: ConversationItem): boolean {
  if (item.kind === "permission") return item.resolved;
  if (item.kind === "elicitation") return item.resolved;
  return item.kind === "thought" || item.kind === "tool" || item.kind === "plan";
}

function emptyTurn(id: string): ConversationTurn {
  return { id, user: null, work: [], agents: [], other: [] };
}

/** Group ACP items into operator turns: user prompt through the next user prompt. */
export function groupConversationTurns(items: ConversationItem[]): ConversationTurn[] {
  const turns: ConversationTurn[] = [];
  let current: ConversationTurn | null = null;

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
