// Centralized API transport. All URLs are relative and same-origin so the
// private hostname, IP address, and any same-origin reverse proxy behave
// identically. Callers receive typed results and normalized errors; they must
// not parse responses or branch on raw status codes themselves.

import {
  assertCockpit,
  assertDetail,
  assertOperationResponse,
  assertPaneSnapshot,
  assertTaskInputResponse,
} from "./contracts";
import { RESTART_POLL_MS, RESTART_TIMEOUT_MS } from "./polling";
import type {
  BrowserCockpitView,
  BrowserPaneSnapshot,
  BrowserTaskDetail,
  OperationRequest,
  OperationResponse,
  StartTaskRequest,
  TaskAnswerRequest,
  TaskInputResponse,
  VersionResponse,
} from "./types";

export type ApiErrorKind =
  | "network"
  | "http"
  | "conflict" // 409 — agent moved on
  | "terminal" // 422 — needs the terminal instead
  | "rate-limit" // 429 — slow down
  | "stale-session" // 401 — browser shell session cookie is missing or stale
  | "incompatible";

export class ApiError extends Error {
  readonly kind: ApiErrorKind;
  readonly status: number | null;
  readonly body: OperationResponse | null;
  constructor(
    kind: ApiErrorKind,
    message: string,
    status: number | null = null,
    body: OperationResponse | null = null,
  ) {
    super(message);
    this.name = "ApiError";
    this.kind = kind;
    this.status = status;
    this.body = body;
  }
}

export function requestId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function classifyStatus(status: number): ApiErrorKind {
  if (status === 401) return "stale-session";
  if (status === 409) return "conflict";
  if (status === 422) return "terminal";
  if (status === 429) return "rate-limit";
  return "http";
}

const GET_OPTIONS: RequestInit = {
  cache: "no-store",
  credentials: "same-origin",
};

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text) return {};
  try {
    return JSON.parse(text);
  } catch {
    return { error: text };
  }
}

async function getJson(path: string): Promise<unknown> {
  let response: Response;
  try {
    response = await fetch(path, GET_OPTIONS);
  } catch (error) {
    throw new ApiError("network", error instanceof Error ? error.message : String(error));
  }
  if (!response.ok) {
    throw new ApiError(classifyStatus(response.status), `HTTP ${response.status}`, response.status);
  }
  return readJson(response);
}

export async function fetchCockpit(): Promise<BrowserCockpitView> {
  const value = await getJson("/api/cockpit");
  return assertCockpit(value);
}

export async function fetchDetail(handle: string): Promise<BrowserTaskDetail> {
  const value = await getJson(`/api/tasks/${encodeURIComponent(handle)}`);
  return assertDetail(value);
}

export async function fetchVersion(): Promise<VersionResponse> {
  const value = await getJson("/api/version");
  return value as VersionResponse;
}

export type PaneResult =
  | { kind: "ok"; snapshot: BrowserPaneSnapshot }
  | { kind: "conflict"; snapshot: BrowserPaneSnapshot }
  | { kind: "missing" };

/** Pane deltas have bespoke status handling: 404 means the endpoint/task is
 * gone (degrade silently), 409 carries a conflict payload (e.g. tmux missing). */
export async function fetchPane(handle: string, since: number): Promise<PaneResult> {
  let response: Response;
  try {
    response = await fetch(
      `/api/tasks/${encodeURIComponent(handle)}/pane?since=${since}`,
      GET_OPTIONS,
    );
  } catch (error) {
    throw new ApiError("network", error instanceof Error ? error.message : String(error));
  }
  if (response.status === 404) return { kind: "missing" };
  if (response.status === 409) {
    return { kind: "conflict", snapshot: assertPaneSnapshot(await readJson(response)) };
  }
  if (!response.ok) {
    throw new ApiError(classifyStatus(response.status), `HTTP ${response.status}`, response.status);
  }
  return { kind: "ok", snapshot: assertPaneSnapshot(await readJson(response)) };
}

async function postJson(path: string, body: unknown): Promise<{ response: Response; payload: unknown }> {
  let response: Response;
  try {
    response = await fetch(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify(body),
    });
  } catch (error) {
    throw new ApiError("network", error instanceof Error ? error.message : String(error));
  }
  const payload = await readJson(response);
  return { response, payload };
}

function errorMessage(payload: unknown, fallback: string): string {
  if (
    typeof payload === "object" &&
    payload !== null &&
    "error" in payload &&
    typeof payload.error === "string"
  ) {
    return payload.error;
  }
  return fallback;
}

/** Operations and task-start return a refreshed cockpit projection; callers
 * replace their projection with it rather than merging optimistically. */
export interface MutationResult {
  ok: boolean;
  response: OperationResponse;
  error?: ApiError;
}

export async function postOperation(req: OperationRequest): Promise<MutationResult> {
  const { response, payload: rawPayload } = await postJson("/api/operations", req);
  const payload = assertOperationResponse(rawPayload);
  if (response.ok) return { ok: true, response: payload };
  return {
    ok: false,
    response: payload,
    error: new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload),
  };
}

export async function startTask(req: StartTaskRequest): Promise<MutationResult> {
  const { response, payload: rawPayload } = await postJson("/api/tasks", req);
  const payload = assertOperationResponse(rawPayload);
  if (response.ok) return { ok: true, response: payload };
  return {
    ok: false,
    response: payload,
    error: new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload),
  };
}

export async function postAnswer(handle: string, req: TaskAnswerRequest): Promise<TaskInputResponse> {
  const { response, payload } = await postJson(`/api/tasks/${encodeURIComponent(handle)}/answer`, req);
  if (!response.ok) {
    throw new ApiError(
      classifyStatus(response.status),
      errorMessage(payload, `HTTP ${response.status}`),
      response.status,
    );
  }
  return assertTaskInputResponse(payload);
}

export async function checkHealth(): Promise<boolean> {
  try {
    const response = await fetch("/api/health", GET_OPTIONS);
    return response.ok;
  } catch {
    return false;
  }
}

/** Poll health until the server answers or the deadline passes. Used after a
 * restart, where a connection drop is expected. */
export async function waitForServerOnline(
  timeoutMs: number = RESTART_TIMEOUT_MS,
  pollMs: number = RESTART_POLL_MS,
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await checkHealth()) return true;
    await new Promise((resolve) => setTimeout(resolve, pollMs));
  }
  return false;
}

export async function restartServer(): Promise<OperationResponse> {
  const { response, payload: rawPayload } = await postJson("/api/server/restart", {});
  const payload = assertOperationResponse(rawPayload);
  if (!response.ok) {
    throw new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload);
  }
  return payload;
}
