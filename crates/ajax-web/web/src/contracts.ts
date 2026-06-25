// Minimal hand-written boundary guards. No schema framework: the backend and
// frontend ship in one binary and the API is same-origin. A guard failure must
// surface a visible "incompatible server response" error — never silently
// coerce data or invent defaults.

import type {
  BrowserCockpitView,
  BrowserPaneSnapshot,
  TaskStatus,
  WebAction,
} from "./types";

export class IncompatibleResponseError extends Error {
  readonly kind = "incompatible" as const;
  constructor(detail: string) {
    super(`Incompatible server response: ${detail}`);
    this.name = "IncompatibleResponseError";
  }
}

const CANONICAL_STATUSES: readonly string[] = ["running", "waiting", "idle", "error"];

export function isTaskStatus(value: unknown): value is TaskStatus {
  return typeof value === "string" && CANONICAL_STATUSES.includes(value);
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function assertAction(value: unknown): WebAction {
  if (!isObject(value) || typeof value.action !== "string") {
    throw new IncompatibleResponseError("action missing string `action` id");
  }
  return value as unknown as WebAction;
}

export function assertCockpit(value: unknown): BrowserCockpitView {
  if (!isObject(value)) {
    throw new IncompatibleResponseError("cockpit is not an object");
  }
  if (!Array.isArray(value.cards)) {
    throw new IncompatibleResponseError("cockpit.cards is not an array");
  }
  for (const card of value.cards) {
    if (!isObject(card)) {
      throw new IncompatibleResponseError("card is not an object");
    }
    if (!isTaskStatus(card.status)) {
      throw new IncompatibleResponseError(`card.status is invalid: ${String(card.status)}`);
    }
    if (!Array.isArray(card.actions)) {
      throw new IncompatibleResponseError("card.actions is not an array");
    }
    card.actions.forEach(assertAction);
  }
  return value as unknown as BrowserCockpitView;
}

export function assertPaneSnapshot(value: unknown): BrowserPaneSnapshot {
  if (!isObject(value)) {
    throw new IncompatibleResponseError("pane snapshot is not an object");
  }
  if (typeof value.sequence !== "number") {
    throw new IncompatibleResponseError("pane.sequence is not a number");
  }
  if (value.lines !== undefined && !Array.isArray(value.lines)) {
    throw new IncompatibleResponseError("pane.lines is not an array");
  }
  return value as unknown as BrowserPaneSnapshot;
}
