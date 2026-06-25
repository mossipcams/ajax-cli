// Transient, presentation-only UI state helpers. Pure functions only — no task
// truth, no optimistic mutation, no lifecycle inference. Every value here is a
// view over server-projected data the browser may select, filter, sort, or
// bound, but never author.

import type { BrowserTaskCard, TaskStatus, WebAction } from "./types";

/** Presentation labels matching the legacy `actionLabel`. Server-provided
 * labels win; otherwise title-case the action id. */
const ACTION_LABELS: Record<string, string> = {
  "fix-ci": "Fix CI",
  "resolve-merge-conflicts": "Resolve conflicts",
};

export function titleCase(value: string): string {
  return value ? value.charAt(0).toUpperCase() + value.slice(1) : value;
}

export function actionLabel(action: WebAction): string {
  if (action.label) return action.label;
  return ACTION_LABELS[action.action] ?? titleCase(action.action);
}

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

export function sortCards(cards: BrowserTaskCard[]): BrowserTaskCard[] {
  return cards
    .slice()
    .sort(
      (a, b) =>
        statusRank(a.status) - statusRank(b.status) ||
        a.qualified_handle.localeCompare(b.qualified_handle),
    );
}

export interface PaneBuffer {
  sequence: number;
  lines: string[];
}

export interface PaneDelta {
  sequence?: number;
  lines?: string[];
}

/**
 * Merge a pane delta into the bounded display buffer. Mirrors the legacy
 * `loadPane` buffering: append only on a strictly newer sequence with lines,
 * preserve lines on an unchanged delta, ignore stale (older) deltas.
 */
export function applyPaneDelta(
  current: PaneBuffer,
  delta: PaneDelta,
  max: number,
): PaneBuffer {
  const incomingSeq = typeof delta.sequence === "number" ? delta.sequence : current.sequence;
  const hasNewLines = Array.isArray(delta.lines) && delta.lines.length > 0;
  if (incomingSeq > current.sequence && hasNewLines) {
    const lines = current.lines.concat(delta.lines as string[]).slice(-max);
    return { sequence: incomingSeq, lines };
  }
  if (incomingSeq >= current.sequence) {
    return { sequence: incomingSeq, lines: current.lines };
  }
  return current;
}

export function isConfirmExpired(entry: { expiresAt: number }, now: number): boolean {
  return now > entry.expiresAt;
}
