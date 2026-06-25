// Minimal hand-written boundary guards. No schema framework: the backend and
// frontend ship in one binary and the API is same-origin. A guard failure must
// surface a visible "incompatible server response" error — never silently
// coerce data or invent defaults.

import type {
  BrowserCockpitView,
  BrowserPaneSnapshot,
  BrowserTaskDetail,
  OperationResponse,
  TaskInputResponse,
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

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function assertOptionalNullableString(
  value: Record<string, unknown>,
  field: string,
): void {
  if (value[field] !== undefined && !isNullableString(value[field])) {
    throw new IncompatibleResponseError(`${field} is not a string or null`);
  }
}

function assertAction(value: unknown): WebAction {
  if (!isObject(value) || typeof value.action !== "string") {
    throw new IncompatibleResponseError("action missing string `action` id");
  }
  if (typeof value.destructive !== "boolean") {
    throw new IncompatibleResponseError("action.destructive is not a boolean");
  }
  if (typeof value.confirmation_required !== "boolean") {
    throw new IncompatibleResponseError("action.confirmation_required is not a boolean");
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
  if (!Array.isArray(value.lines) || value.lines.some((line) => typeof line !== "string")) {
    throw new IncompatibleResponseError("pane.lines is not an array of strings");
  }
  if (typeof value.truncated !== "boolean") {
    throw new IncompatibleResponseError("pane.truncated is not a boolean");
  }
  if (typeof value.tmux_exists !== "boolean") {
    throw new IncompatibleResponseError("pane.tmux_exists is not a boolean");
  }
  if (value.state !== null) {
    if (!isObject(value.state)) {
      throw new IncompatibleResponseError("pane.state is not an object or null");
    }
    if (typeof value.state.kind !== "string" || typeof value.state.summary !== "string") {
      throw new IncompatibleResponseError("pane.state identity fields are invalid");
    }
    if (!isNullableString(value.state.command) || !isNullableString(value.state.prompt)) {
      throw new IncompatibleResponseError("pane.state command or prompt is invalid");
    }
    if (!Array.isArray(value.state.choices)) {
      throw new IncompatibleResponseError("pane.state.choices is not an array");
    }
    for (const choice of value.state.choices) {
      if (
        !isObject(choice) ||
        typeof choice.index !== "number" ||
        typeof choice.label !== "string" ||
        !["affirm", "deny", "neutral"].includes(String(choice.role))
      ) {
        throw new IncompatibleResponseError("pane choice is malformed");
      }
    }
    if (
      value.state.confidence !== undefined &&
      value.state.confidence !== "high" &&
      value.state.confidence !== "low"
    ) {
      throw new IncompatibleResponseError("pane.state.confidence is invalid");
    }
    assertOptionalNullableString(value.state, "fingerprint");
    if (typeof value.state.answerable !== "boolean") {
      throw new IncompatibleResponseError("pane.state.answerable is not a boolean");
    }
  }
  return value as unknown as BrowserPaneSnapshot;
}

export function assertDetail(value: unknown): BrowserTaskDetail {
  if (!isObject(value)) {
    throw new IncompatibleResponseError("task detail is not an object");
  }
  if (typeof value.qualified_handle !== "string") {
    throw new IncompatibleResponseError("detail.qualified_handle is not a string");
  }
  if (!isTaskStatus(value.status)) {
    throw new IncompatibleResponseError(`detail.status is invalid: ${String(value.status)}`);
  }
  if (!Array.isArray(value.actions)) {
    throw new IncompatibleResponseError("detail.actions is not an array");
  }
  value.actions.forEach(assertAction);
  if (!Array.isArray(value.agent_attempts)) {
    throw new IncompatibleResponseError("detail.agent_attempts is not an array");
  }
  return value as unknown as BrowserTaskDetail;
}

export function assertOperationResponse(value: unknown): OperationResponse {
  if (!isObject(value)) {
    throw new IncompatibleResponseError("operation response is not an object");
  }
  if (typeof value.ok !== "boolean") {
    throw new IncompatibleResponseError("operation.ok is not a boolean");
  }
  if (value.request_id !== undefined && typeof value.request_id !== "string") {
    throw new IncompatibleResponseError("operation.request_id is not a string");
  }
  if (value.state_changed !== undefined && typeof value.state_changed !== "boolean") {
    throw new IncompatibleResponseError("operation.state_changed is not a boolean");
  }
  assertOptionalNullableString(value, "output");
  assertOptionalNullableString(value, "error");
  if (value.restarting !== undefined && typeof value.restarting !== "boolean") {
    throw new IncompatibleResponseError("operation.restarting is not a boolean");
  }
  if (value.cockpit !== undefined) {
    assertCockpit(value.cockpit);
  }
  return value as unknown as OperationResponse;
}

export function assertTaskInputResponse(value: unknown): TaskInputResponse {
  if (!isObject(value) || typeof value.sequence_hint !== "number") {
    throw new IncompatibleResponseError("task input response is malformed");
  }
  return value as unknown as TaskInputResponse;
}
