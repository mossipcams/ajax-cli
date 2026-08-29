// Centralized API transport. All URLs are relative and same-origin so the
// private hostname, IP address, and any same-origin reverse proxy behave
// identically. Callers receive typed results and normalized errors; they must
// not parse responses or branch on raw status codes themselves.

import {
  assertCockpit,
  assertDetail,
  assertOperationResponse,
} from "./contracts";
import { RESTART_POLL_MS, RESTART_TIMEOUT_MS, TEST_IN_STABLE_TIMEOUT_MS, GET_REQUEST_TIMEOUT_MS, DIFF_REQUEST_TIMEOUT_MS } from "./polling";
import {
  ApiError,
  type ApiErrorKind,
  type BrowserCockpitView,
  type BrowserTaskDetail,
  type DevDeployResponse,
  type OperationRequest,
  type OperationResponse,
  type PullRequestView,
  type PushTestSubscription,
  type PushVapidResponse,
  type RuntimeStatusResponse,
  type StartTaskRequest,
  type TaskDiffView,
  type VersionResponse,
} from "./types";

export { ApiError, type ApiErrorKind };

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

function getOptions(timeoutMs: number = GET_REQUEST_TIMEOUT_MS, querySignal?: AbortSignal): RequestInit {
  const signal = querySignal
    ? AbortSignal.any([AbortSignal.timeout(timeoutMs), querySignal])
    : AbortSignal.timeout(timeoutMs);
  return {
    cache: "no-store",
    credentials: "same-origin",
    signal,
  };
}

/** Sent on cockpit polls only while the document is visible — same request, no extra RTT. */
export const AJAX_FOREGROUND_HEADER = "X-Ajax-Foreground";

function documentIsForeground(): boolean {
  return typeof document !== "undefined" && document.visibilityState === "visible";
}

function cockpitGetOptions(timeoutMs: number = GET_REQUEST_TIMEOUT_MS): RequestInit {
  const options = getOptions(timeoutMs);
  if (!documentIsForeground()) return options;
  return {
    ...options,
    headers: {
      [AJAX_FOREGROUND_HEADER]: "1",
    },
  };
}

function sessionRenewOptions(): RequestInit {
  return {
    method: "POST",
    cache: "no-store",
    credentials: "same-origin",
    signal: AbortSignal.timeout(GET_REQUEST_TIMEOUT_MS),
  };
}

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

/** Re-issues the browser session cookie. Concurrent callers share one in-flight
 * request. Exported for the terminal socket, which cannot see the handshake
 * status and so renews on any dial that failed to open. */
export async function renewBrowserSession(): Promise<void> {
  if (!browserSessionRenewal) {
    browserSessionRenewal = (async () => {
      let response: Response;
      try {
        response = await fetch("/api/session", sessionRenewOptions());
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

async function getJson(
  path: string,
  timeoutMs: number = GET_REQUEST_TIMEOUT_MS,
  querySignal?: AbortSignal,
): Promise<unknown> {
  const response = await fetchProtectedWithSessionRenewal(path, getOptions(timeoutMs, querySignal));
  if (!response.ok) {
    throw new ApiError(classifyStatus(response.status), `HTTP ${response.status}`, response.status);
  }
  return readJson(response);
}

async function getJsonPreferringErrorBody(
  path: string,
  timeoutMs: number,
): Promise<unknown> {
  const response = await fetchProtectedWithSessionRenewal(path, getOptions(timeoutMs));
  if (!response.ok) {
    const payload = await readJson(response);
    throw new ApiError(
      classifyStatus(response.status),
      errorMessage(payload, `HTTP ${response.status}`),
      response.status,
    );
  }
  return readJson(response);
}

export async function fetchCockpit(): Promise<BrowserCockpitView> {
  const response = await fetchProtectedWithSessionRenewal("/api/cockpit", cockpitGetOptions());
  if (!response.ok) {
    throw new ApiError(classifyStatus(response.status), `HTTP ${response.status}`, response.status);
  }
  return assertCockpit(await readJson(response));
}

export async function fetchDetail(handle: string): Promise<BrowserTaskDetail> {
  const value = await getJson(`/api/tasks/${encodeURIComponent(handle)}`);
  return assertDetail(value);
}

export async function fetchTaskPullRequests(handle: string): Promise<PullRequestView[]> {
  const value = await getJsonPreferringErrorBody(
    `/api/tasks/${encodeURIComponent(handle)}/pull-requests`,
    DIFF_REQUEST_TIMEOUT_MS,
  );
  if (
    typeof value !== "object" ||
    value === null ||
    !("pull_requests" in value) ||
    !Array.isArray((value as { pull_requests: unknown }).pull_requests)
  ) {
    throw new ApiError("incompatible", "invalid pull-requests payload");
  }
  return (value as { pull_requests: PullRequestView[] }).pull_requests;
}

export async function fetchTaskDiff(
  handle: string,
  options: { pr?: number; local?: boolean } = {},
): Promise<TaskDiffView> {
  const params = new URLSearchParams();
  if (options.local) params.set("local", "1");
  else if (options.pr !== undefined) params.set("pr", String(options.pr));
  const query = params.toString();
  const path = `/api/tasks/${encodeURIComponent(handle)}/diff${query ? `?${query}` : ""}`;
  const value = await getJsonPreferringErrorBody(path, DIFF_REQUEST_TIMEOUT_MS);
  if (typeof value !== "object" || value === null || !("files" in value) || !("source" in value)) {
    throw new ApiError("incompatible", "invalid diff payload");
  }
  return value as TaskDiffView;
}

export async function fetchVersion(): Promise<VersionResponse> {
  const value = await getJson("/api/version");
  return value as VersionResponse;
}

export async function fetchRuntimeStatus(): Promise<RuntimeStatusResponse> {
  const value = await getJson("/api/server/runtime");
  return value as RuntimeStatusResponse;
}

export async function fetchPushVapidPublicKey(): Promise<PushVapidResponse> {
  const value = await getJson("/api/push/vapid");
  if (
    typeof value !== "object" ||
    value === null ||
    !("public_key" in value) ||
    typeof (value as PushVapidResponse).public_key !== "string"
  ) {
    throw new ApiError("incompatible", "invalid push vapid payload");
  }
  return value as PushVapidResponse;
}

export async function sendPushSubscribe(subscription: PushTestSubscription): Promise<void> {
  const { response, payload } = await postJson("/api/push/subscribe", {
    endpoint: subscription.endpoint,
    keys: subscription.keys,
  });
  if (!response.ok) {
    throw new ApiError(
      classifyStatus(response.status),
      errorMessage(payload, `HTTP ${response.status}`),
      response.status,
    );
  }
}

export async function sendPushUnsubscribe(
  endpoint?: string,
  options: { all?: boolean } = {},
): Promise<void> {
  const body = options.all
    ? { all: true }
    : endpoint
      ? { endpoint }
      : { all: true };
  const response = await fetchProtectedWithSessionRenewal("/api/push/subscribe", {
    method: "DELETE",
    headers: { "content-type": "application/json" },
    cache: "no-store",
    credentials: "same-origin",
    body: JSON.stringify(body),
  });
  const payload = await readJson(response);
  if (!response.ok) {
    throw new ApiError(
      classifyStatus(response.status),
      errorMessage(payload, `HTTP ${response.status}`),
      response.status,
    );
  }
}

export async function sendPushTest(subscription: PushTestSubscription): Promise<void> {
  const { response, payload } = await postJson("/api/push/test", subscription);
  if (!response.ok) {
    throw new ApiError(
      classifyStatus(response.status),
      errorMessage(payload, `HTTP ${response.status}`),
      response.status,
    );
  }
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

function operationErrorCode(payload: OperationResponse): string | null {
  return typeof payload.code === "string" && payload.code.length > 0 ? payload.code : null;
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
    error: new ApiError(
      classifyStatus(response.status),
      payload.error || `HTTP ${response.status}`,
      response.status,
      payload,
      operationErrorCode(payload),
    ),
  };
}

export async function postOperation(req: OperationRequest): Promise<MutationResult> {
  return postMutation("/api/operations", req);
}

export async function startTask(req: StartTaskRequest): Promise<MutationResult> {
  return postMutation("/api/tasks", req);
}

/** Move an existing ACP-backed task to another harness (and model). */
export async function swapTaskAgent(
  handle: string,
  agent: string,
  model?: string,
): Promise<MutationResult> {
  return postMutation(`/api/tasks/${encodeURIComponent(handle)}`, {
    agent,
    ...(model ? { model } : {}),
  });
}

export async function checkHealth(): Promise<boolean> {
  try {
    const response = await fetch("/api/health", getOptions());
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

async function peekVersion(): Promise<string | null> {
  try {
    const response = await fetch("/api/version", getOptions());
    if (!response.ok) return null;
    const value = await readJson(response);
    if (
      typeof value === "object" &&
      value !== null &&
      "version" in value &&
      typeof (value as VersionResponse).version === "string"
    ) {
      return (value as VersionResponse).version;
    }
    return null;
  } catch {
    return null;
  }
}

/** Poll until the server has restarted: require a down edge or version change,
 * then two consecutive healthy checks so we do not reload into a dying process. */
export async function waitForServerRestart(options?: {
  timeoutMs?: number;
  pollMs?: number;
  previousVersion?: string | null;
}): Promise<boolean> {
  const timeoutMs = options?.timeoutMs ?? RESTART_TIMEOUT_MS;
  const pollMs = options?.pollMs ?? RESTART_POLL_MS;
  const previousVersion = options?.previousVersion ?? null;
  const deadline = Date.now() + timeoutMs;
  let restartObserved = false;
  let consecutiveHealthy = 0;

  while (Date.now() < deadline) {
    const healthy = await checkHealth();

    if (!healthy) {
      restartObserved = true;
      consecutiveHealthy = 0;
    } else if (!restartObserved) {
      if (previousVersion != null) {
        const current = await peekVersion();
        if (current !== null && current !== previousVersion) {
          restartObserved = true;
          consecutiveHealthy = 1;
        }
      }
    } else {
      consecutiveHealthy += 1;
      if (consecutiveHealthy >= 2) return true;
    }

    await new Promise((resolve) => setTimeout(resolve, pollMs));
  }
  return false;
}

export async function restartServer(): Promise<OperationResponse> {
  const { response, payload: rawPayload } = await postJson("/api/server/restart", {});
  const payload = assertOperationResponse(rawPayload);
  if (!response.ok) {
    throw new ApiError(
      classifyStatus(response.status),
      payload.error || `HTTP ${response.status}`,
      response.status,
      payload,
      operationErrorCode(payload),
    );
  }
  return payload;
}

export async function startTestInStable(): Promise<OperationResponse> {
  const { response, payload: rawPayload } = await postJson("/api/server/test-in-stable", {});
  const payload = assertOperationResponse(rawPayload);
  if (!response.ok) {
    throw new ApiError(
      classifyStatus(response.status),
      payload.error || `HTTP ${response.status}`,
      response.status,
      payload,
      operationErrorCode(payload),
    );
  }
  return payload;
}

export async function updateServer(): Promise<OperationResponse> {
  const { response, payload: rawPayload } = await postJson("/api/server/update", {});
  const payload = assertOperationResponse(rawPayload);
  if (!response.ok) {
    throw new ApiError(
      classifyStatus(response.status),
      payload.error || `HTTP ${response.status}`,
      response.status,
      payload,
      operationErrorCode(payload),
    );
  }
  return payload;
}

export { TEST_IN_STABLE_TIMEOUT_MS };

export async function fetchDevDeploy(querySignal?: AbortSignal): Promise<DevDeployResponse> {
  const value = await getJson("/api/dev-deploy", GET_REQUEST_TIMEOUT_MS, querySignal);
  return value as DevDeployResponse;
}

export async function startDevDeploy(taskHandle: string): Promise<DevDeployResponse> {
  const { response, payload } = await postJson("/api/dev-deploy", {
    task_handle: taskHandle,
  });
  const body = payload as DevDeployResponse;
  if (!response.ok) {
    throw new ApiError(
      classifyStatus(response.status),
      body.error || `HTTP ${response.status}`,
      response.status,
    );
  }
  return body;
}

/** Fresh allowlisted id for one terminal connection controller lifetime.
 * Not sessionStorage: duplicated tabs copy sessionStorage and would share a
 * tmux ephemeral viewport (competing resize/input). `connectTaskTerminal`
 * calls this once and reuses the value across that controller's redials. */
export function createTerminalClientId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function taskTerminalWebSocketUrl(
  handle: string,
  seedHistory = true,
  clientId?: string,
): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const base = `${protocol}//${window.location.host}/api/tasks/${encodeURIComponent(handle)}/terminal`;
  const params = new URLSearchParams();
  if (clientId) params.set("client", clientId);
  if (!seedHistory) params.set("seed", "0");
  const query = params.toString();
  return query ? `${base}?${query}` : base;
}

export function openTaskTerminalSocket(
  handle: string,
  seedHistory = true,
  clientId?: string,
): WebSocket {
  return new WebSocket(taskTerminalWebSocketUrl(handle, seedHistory, clientId));
}
