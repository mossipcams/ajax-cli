import type { ConversationItem } from "../session/public";

export type TurnRow =
  | { kind: "work"; id: string; items: ConversationItem[] }
  | { kind: "item"; id: string; item: ConversationItem };

export interface ConversationTurn {
  id: string;
  user: ConversationItem | null;
  rows: TurnRow[];
}

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
