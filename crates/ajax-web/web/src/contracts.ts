// Minimal hand-written boundary guards. No schema framework: the backend and
// frontend ship in one binary and the API is same-origin. A guard failure must
// surface a visible "incompatible server response" error — never silently
// coerce data or invent defaults.

import type {
  BrowserCockpitView,
  BrowserTaskDetail,
  OperationResponse,
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
  if (typeof value.label !== "string") {
    throw new IncompatibleResponseError("action.label is not a string");
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
  if (
    value.confirmation_token !== undefined &&
    typeof value.confirmation_token !== "string"
  ) {
    throw new IncompatibleResponseError("operation.confirmation_token is not a string");
  }
  if (value.restarting !== undefined && typeof value.restarting !== "boolean") {
    throw new IncompatibleResponseError("operation.restarting is not a boolean");
  }
  if (value.cockpit !== undefined) {
    assertCockpit(value.cockpit);
  }
  return value as unknown as OperationResponse;
}
