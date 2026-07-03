// Centralized API transport. All URLs are relative and same-origin so the
// private hostname, IP address, and any same-origin reverse proxy behave
// identically. Callers receive typed results and normalized errors; they must
// not parse responses or branch on raw status codes themselves.

import {
  assertCockpit,
  assertDetail,
  assertOperationResponse,
} from "./contracts";
import { RESTART_POLL_MS, RESTART_TIMEOUT_MS } from "./polling";
import type {
  BrowserCockpitView,
  BrowserTaskDetail,
  OperationRequest,
  OperationResponse,
  StartTaskRequest,
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

const SESSION_RENEW_OPTIONS: RequestInit = {
  method: "POST",
  cache: "no-store",
  credentials: "same-origin",
};

let browserSessionRenewal: Promise<void> | null = null;

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text) return {};
  try {
    return JSON.parse(text);
  } catch {
    return { error: text };
  }
}

async function renewBrowserSession(): Promise<void> {
  if (!browserSessionRenewal) {
    browserSessionRenewal = (async () => {
      let response: Response;
      try {
        response = await fetch("/api/session", SESSION_RENEW_OPTIONS);
      } catch (error) {
        throw new ApiError(
          "stale-session",
          error instanceof Error ? error.message : String(error),
          null,
        );
      }
      if (!response.ok) {
        throw new ApiError("stale-session", `HTTP ${response.status}`, response.status);
      }
      const payload = await readJson(response);
      const renewed =
        typeof payload === "object" && payload !== null && "ok" in payload && payload.ok === true;
      if (!renewed) {
        throw new ApiError(
          "stale-session",
          errorMessage(payload, "browser session renewal failed"),
          response.status,
        );
      }
    })().finally(() => {
      browserSessionRenewal = null;
    });
  }
  return browserSessionRenewal;
}

async function fetchProtectedWithSessionRenewal(path: string, init: RequestInit): Promise<Response> {
  let response: Response;
  try {
    response = await fetch(path, init);
  } catch (error) {
    throw new ApiError("network", error instanceof Error ? error.message : String(error));
  }
  if (response.status !== 401) return response;

  await renewBrowserSession();
  try {
    const retryResponse = await fetch(path, init);
    if (retryResponse.status === 401) {
      throw new ApiError("stale-session", "HTTP 401", 401);
    }
    return retryResponse;
  } catch (error) {
    if (error instanceof ApiError) throw error;
    throw new ApiError("network", error instanceof Error ? error.message : String(error));
  }
}

async function getJson(path: string): Promise<unknown> {
  const response = await fetchProtectedWithSessionRenewal(path, GET_OPTIONS);
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

async function postJson(path: string, body: unknown): Promise<{ response: Response; payload: unknown }> {
  const response = await fetchProtectedWithSessionRenewal(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    cache: "no-store",
    credentials: "same-origin",
    body: JSON.stringify(body),
  });
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

async function postMutation(path: string, req: unknown): Promise<MutationResult> {
  const { response, payload: rawPayload } = await postJson(path, req);
  const payload = assertOperationResponse(rawPayload);
  if (response.ok) return { ok: true, response: payload };
  return {
    ok: false,
    response: payload,
    error: new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload),
  };
}

export async function postOperation(req: OperationRequest): Promise<MutationResult> {
  return postMutation("/api/operations", req);
}

export async function startTask(req: StartTaskRequest): Promise<MutationResult> {
  return postMutation("/api/tasks", req);
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

export async function restartServer(confirmation_token?: string): Promise<OperationResponse> {
  const { response, payload: rawPayload } = await postJson(
    "/api/server/restart",
    confirmation_token ? { confirmation_token } : {},
  );
  const payload = assertOperationResponse(rawPayload);
  if (!response.ok && payload.confirmation_token) {
    return payload;
  }
  if (!response.ok) {
    throw new ApiError(classifyStatus(response.status), payload.error || `HTTP ${response.status}`, response.status, payload);
  }
  return payload;
}

export function taskTerminalWebSocketUrl(handle: string): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/api/tasks/${encodeURIComponent(handle)}/terminal`;
}

export function openTaskTerminalSocket(handle: string): WebSocket {
  return new WebSocket(taskTerminalWebSocketUrl(handle));
}
