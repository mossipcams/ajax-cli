import type { ConversationItem } from "../session/public";

/** A turn reads in the order it happened: what the agent said, the work it did
 * between two things it said, an ask it is still waiting on. A run of adjacent
 * work items collapses into one disclosure; prose in the middle ends the run. */
export type TurnRow =
  | { kind: "work"; id: string; items: ConversationItem[] }
  | { kind: "item"; id: string; item: ConversationItem };

export interface ConversationTurn {
  id: string;
  user: ConversationItem | null;
  rows: TurnRow[];
}

/** Protocol detail belongs behind the disclosure; only what the operator must
 * act on stays in the conversation. An unanswered permission or elicitation is
 * an action, so it stays out here until it is resolved and becomes history. */
function isWorkItem(item: ConversationItem): boolean {
  if (item.kind === "permission") return item.resolved;
  if (item.kind === "elicitation") return item.resolved;
  return item.kind === "thought" || item.kind === "tool" || item.kind === "plan";
}

function append(turn: ConversationTurn, item: ConversationItem): void {
  if (!isWorkItem(item)) {
    turn.rows.push({ kind: "item", id: item.id, item });
    return;
  }
  const last = turn.rows[turn.rows.length - 1];
  if (last?.kind === "work") last.items.push(item);
  else turn.rows.push({ kind: "work", id: `work:${item.id}`, items: [item] });
}

/** Group ACP items into operator turns: user prompt through the next user prompt. */
export function groupConversationTurns(items: ConversationItem[]): ConversationTurn[] {
  const turns: ConversationTurn[] = [];
  let current: ConversationTurn | null = null;

  for (const item of items) {
    if (item.kind === "prose" && item.role === "user") {
      current = { id: item.id, user: item, rows: [] };
      turns.push(current);
      continue;
    }

    if (!current) {
      // Replay can open on agent output with no prompt in front of it. Those
      // rows share one headless turn rather than one turn each.
      const last = turns[turns.length - 1];
      if (last && !last.user) {
        current = last;
      } else {
        current = { id: item.id, user: null, rows: [] };
        turns.push(current);
      }
    }

    append(current, item);
  }

  return turns;
}
