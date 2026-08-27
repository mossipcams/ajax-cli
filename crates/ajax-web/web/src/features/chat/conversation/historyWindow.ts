import type { ConversationItem } from "../session/public";

/** Recent rows painted on first open; full transcript stays in session reducer. */
export const DEFAULT_HISTORY_WINDOW = 150;

/** Reveal batch when scrolling up or tapping Load earlier. */
export const HISTORY_REVEAL_BATCH = 50;

function isUserTurnStart(item: ConversationItem): boolean {
  return item.kind === "prose" && item.role === "user";
}

/** Indices where a turn may start without severing grouped rows. */
export function turnStartIndices(items: ConversationItem[]): number[] {
  if (items.length === 0) return [];

  const indices: number[] = [0];
  for (let index = 1; index < items.length; index += 1) {
    if (isUserTurnStart(items[index]!)) indices.push(index);
  }
  return indices;
}

/**
 * Snap the initial window start forward to a turn boundary within the cap so
 * the first paint never opens mid-turn.
 */
export function historyWindowStart(items: ConversationItem[], cap: number): number {
  if (items.length <= cap) return 0;

  const naive = items.length - cap;
  const boundaries = turnStartIndices(items);
  for (const boundary of boundaries) {
    if (boundary >= naive) return boundary;
  }
  return naive;
}

/** Expand backward toward `target`, snapping to the nearest turn start below the window. */
export function snapRevealStart(
  items: ConversationItem[],
  windowStart: number,
  batch: number,
): number {
  if (windowStart === 0) return 0;

  const target = Math.max(0, windowStart - batch);
  const boundaries = turnStartIndices(items);

  let chosen: number | null = null;
  for (const boundary of boundaries) {
    if (boundary >= target && boundary < windowStart) {
      chosen = chosen === null ? boundary : Math.min(chosen, boundary);
    }
  }
  if (chosen !== null) return chosen;

  return target;
}
