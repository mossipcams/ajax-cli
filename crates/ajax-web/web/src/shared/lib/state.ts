// Transient, presentation-only UI state helpers. Pure functions only — no task
// truth, no optimistic mutation, no lifecycle inference. Every value here is a
// view over server-projected data the browser may select, filter, sort, or
// bound, but never author.

import type { BrowserTaskCard, TaskStatus } from "./types";

/** Status tone + label for badges/dots. The browser only renders the canonical
 * four-state contract; Rust owns derivation. */
export interface StatusMeta {
  tone: TaskStatus;
  label: string;
}

const STATUS_LABELS: Record<TaskStatus, string> = {
  running: "Running",
  waiting: "Waiting",
  idle: "Idle",
  error: "Error",
};

export function statusMeta(status: string): StatusMeta {
  const key = (status || "").toLowerCase();
  if (!(STATUS_ORDER as string[]).includes(key)) {
    console.warn(`[ajax] unknown task status: ${JSON.stringify(status)} — defaulting to idle`);
  }
  const tone = (STATUS_ORDER as string[]).includes(key) ? (key as TaskStatus) : "idle";
  return { tone, label: STATUS_LABELS[tone] };
}

export function severityBucket(value: number): "high" | "medium" | "low" {
  if (value <= 2) return "high";
  if (value <= 3) return "medium";
  return "low";
}

/** Presentation-only ordering. NOT a priority policy (that lives in Rust). */
export const STATUS_ORDER: TaskStatus[] = ["running", "waiting", "error", "idle"];

export function statusRank(status: string): number {
  const index = STATUS_ORDER.indexOf((status || "").toLowerCase() as TaskStatus);
  return index === -1 ? STATUS_ORDER.length : index;
}

export function filterByProject(
  cards: BrowserTaskCard[],
  project: string | null,
): BrowserTaskCard[] {
  if (!project) return cards;
  return cards.filter((card) => card.repo === project);
}

export function sortCards(
  cards: BrowserTaskCard[],
  previousOrder: readonly string[] = [],
): BrowserTaskCard[] {
  const prevIndex = new Map(previousOrder.map((handle, index) => [handle, index]));
  return cards
    .slice()
    .sort((a, b) => {
      const byStatus = statusRank(a.status) - statusRank(b.status);
      if (byStatus !== 0) return byStatus;

      const aPrev = prevIndex.get(a.qualified_handle);
      const bPrev = prevIndex.get(b.qualified_handle);
      if (aPrev !== undefined && bPrev !== undefined && aPrev !== bPrev) {
        return aPrev - bPrev;
      }
      if (aPrev !== undefined && bPrev === undefined) return -1;
      if (aPrev === undefined && bPrev !== undefined) return 1;

      return (
        (b.last_activity_unix_secs ?? 0) - (a.last_activity_unix_secs ?? 0) ||
        a.qualified_handle.localeCompare(b.qualified_handle)
      );
    });
}

export function isConfirmExpired(entry: { expiresAt: number }, now: number): boolean {
  return now > entry.expiresAt;
}

/** A running task with no activity for this long reads as quiet/stuck. */
export const QUIET_THRESHOLD_SECS = 240; // 4 minutes

/** True when a running task has gone silent past the quiet threshold — the
 * "running but wedged" signal a plain status count hides. Unset activity (0)
 * never counts as quiet. */
export function isQuiet(card: BrowserTaskCard, nowSecs: number): boolean {
  return (
    card.status === "running" &&
    card.last_activity_unix_secs > 0 &&
    nowSecs - card.last_activity_unix_secs >= QUIET_THRESHOLD_SECS
  );
}

export type ActiveStatus = Exclude<TaskStatus, "idle">;

export interface FleetSegment {
  status: ActiveStatus;
  count: number;
}

/** Muster Bar order: faults first, then what's blocked on you, then the healthy
 * body. Idle is inventory, not fleet health, so it never gets a segment. */
const ACTIVE_ORDER: ActiveStatus[] = ["error", "waiting", "running"];

/** Active-fleet composition for the Muster Bar. Only nonzero states appear, so
 * an accent never shows for an empty state (Accent Rarity holds by construction). */
export function fleetSegments(cards: BrowserTaskCard[]): FleetSegment[] {
  return ACTIVE_ORDER.map((status) => ({
    status,
    count: cards.filter((c) => c.status === status).length,
  })).filter((segment) => segment.count > 0);
}

/** Repos with at least one faulted task — drives the project-pill fault dot. */
export function reposWithFault(cards: BrowserTaskCard[]): Set<string> {
  return new Set(cards.filter((c) => c.status === "error").map((c) => c.repo));
}

/** Compact relative timestamp for glanceable metadata: "now", "5m ago",
 * "2d ago". Unset (zero) timestamps render as "—"; clock skew clamps to "now". */
export function relativeTime(unixSecs: number, nowSecs: number): string {
  if (!unixSecs) return "—";
  const delta = Math.max(0, nowSecs - unixSecs);
  if (delta < 60) return "now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
}

/** Compact duration: "42s", "3m", "1h 12m". */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  if (total < 60) return `${total}s`;
  if (total < 3600) return `${Math.floor(total / 60)}m`;
  return `${Math.floor(total / 3600)}h ${Math.floor((total % 3600) / 60)}m`;
}
