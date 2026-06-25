// Centralized API transport. All URLs are relative and same-origin so the
// private hostname, IP address, and any same-origin reverse proxy behave
// identically. Callers receive typed results and normalized errors; they must
// not parse responses or branch on raw status codes themselves.

import { assertCockpit, IncompatibleResponseError } from "./contracts";
import type {
  BrowserCockpitView,
  BrowserTaskDetail,
  OperationRequest,
  OperationResponse,
  StartTaskRequest,
  TaskAnswerRequest,
  VersionResponse,
} from "./types";

export type ApiErrorKind =
  | "network"
  | "http"
  | "conflict" // 409 — agent moved on
  | "terminal" // 422 — needs the terminal instead
  | "rate-limit" // 429 — slow down
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
  if (status === 409) return "conflict";
  if (status === 422) return "terminal";
  if (status === 429) return "rate-limit";
  return "http";
}

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
    response = await fetch(path, { cache: "no-store" });
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
  if (value instanceof IncompatibleResponseError) throw value;
  return assertCockpit(value);
}

export async function fetchDetail(handle: string): Promise<BrowserTaskDetail> {
  const value = await getJson(`/api/tasks/${encodeURIComponent(handle)}`);
  return value as BrowserTaskDetail;
}

export async function fetchVersion(): Promise<VersionResponse> {
  const value = await getJson("/api/version");
  return value as VersionResponse;
}

async function postJson(path: string, body: unknown): Promise<{ response: Response; payload: OperationResponse }> {
  let response: Response;
  try {
    response = await fetch(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      body: JSON.stringify(body),
    });
  } catch (error) {
    throw new ApiError("network", error instanceof Error ? error.message : String(error));
  }
  const payload = (await readJson(response)) as OperationResponse;
  return { response, payload };
}

/** Operations and task-start return a refreshed cockpit projection; callers
 * replace their projection with it rather than merging optimistically. */
export interface MutationResult {
  ok: boolean;
  response: OperationResponse;
  error?: ApiError;
}

export async function postOperation(req: OperationRequest): Promise<MutationResult> {
  const { response, payload } = await postJson("/api/operations", req);
  if (response.ok) return { ok: true, response: payload };
  return {
    ok: false,
    response: payload,
    error: new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload),
  };
}

export async function startTask(req: StartTaskRequest): Promise<MutationResult> {
  const { response, payload } = await postJson("/api/tasks", req);
  if (response.ok) return { ok: true, response: payload };
  return {
    ok: false,
    response: payload,
    error: new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload),
  };
}

export async function postAnswer(handle: string, req: TaskAnswerRequest): Promise<OperationResponse> {
  const { response, payload } = await postJson(`/api/tasks/${encodeURIComponent(handle)}/answer`, req);
  if (!response.ok) {
    throw new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload);
  }
  return payload;
}

export async function restartServer(): Promise<OperationResponse> {
  const { response, payload } = await postJson("/api/server/restart", {});
  if (!response.ok) {
    throw new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload);
  }
  return payload;
}
