// What a settled turn looks like when it is one line in a list.
//
// The operator's most common arrival is returning to a task that ran while they
// were not looking, so history has to answer "what happened" without being
// read. A turn's own prompt is its title, and what came of it is a single
// summary line — the same counted summary the activity disclosure already
// shows, plus whether the turn failed and what it changed on disk.

import { activitySummary, diffLines } from "../activity/public";
import type { ConversationItem } from "../session/public";
import type { ConversationTurn } from "./groupTurns";

export interface ChangedFile {
  path: string;
  added: number;
  removed: number;
}

export interface TurnDigest {
  /** The prompt, which is also the turn's title. */
  ask: string | null;
  /** Counted work summary, or null when the turn did no work. */
  outcome: string | null;
  changed: ChangedFile[];
  failed: boolean;
  /** An ask the operator still owes an answer to. */
  awaiting: boolean;
}

function workItems(turn: ConversationTurn): ConversationItem[] {
  return turn.rows.flatMap((row) => (row.kind === "work" ? row.items : []));
}

function looseItems(turn: ConversationTurn): ConversationItem[] {
  return turn.rows.flatMap((row) => (row.kind === "item" ? [row.item] : []));
}

/** Files the turn wrote, with the shape of the change. A path edited twice is
 * one entry: the operator asks "what did it touch", not "how many times". */
export function changedFiles(turn: ConversationTurn): ChangedFile[] {
  const byPath = new Map<string, ChangedFile>();
  for (const item of workItems(turn)) {
    if (item.kind !== "tool") continue;
    for (const content of item.call.content) {
      if (content.type !== "diff") continue;
      const lines = diffLines(content.oldText ?? "", content.newText);
      const entry = byPath.get(content.path) ?? { path: content.path, added: 0, removed: 0 };
      entry.added += lines.filter((line) => line.sign === "+").length;
      entry.removed += lines.filter((line) => line.sign === "-").length;
      byPath.set(content.path, entry);
    }
  }
  return [...byPath.values()];
}

export function turnDigest(turn: ConversationTurn): TurnDigest {
  const work = workItems(turn);
  const loose = looseItems(turn);
  return {
    ask: turn.user?.kind === "prose" ? turn.user.text.trim() || null : null,
    outcome: work.length ? activitySummary(work) : null,
    changed: changedFiles(turn),
    failed:
      work.some((item) => item.kind === "tool" && item.call.status === "failed") ||
      loose.some((item) => item.kind === "note" && item.tone === "error"),
    awaiting: loose.some(
      (item) =>
        (item.kind === "permission" || item.kind === "elicitation") && !item.resolved,
    ),
  };
}

/** Which turns open themselves: the one the operator came back for, the one in
 * flight, and one that still wants an answer.
 *
 * Not failures. Opening every failed turn sounds right and reads wrong — a
 * failed run expands its whole tool card and output, so three turns of history
 * become one, which is the density this list exists for. A failure is legible
 * as a line: the red mark and an outcome that ends in `1 failed`. */
export function opensByDefault(
  digest: TurnDigest,
  position: { isLast: boolean; isLive: boolean },
): boolean {
  return position.isLast || position.isLive || digest.awaiting;
}
