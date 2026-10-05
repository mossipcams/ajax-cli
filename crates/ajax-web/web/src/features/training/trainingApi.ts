// Typed client for the same-origin training endpoints. Transport mirrors
// shared/lib/api.ts (same-origin credentials, 401 -> renewBrowserSession ->
// single retry); auth itself is delegated to the shared helper, not re-implemented.
// profile_details[name].serving on the host is a descriptive string (e.g. "ctx
// 122880, 2 slots"), never a boolean; serve state derives from active_profile +
// runtime_up in the UI.
import { ApiError, renewBrowserSession } from "@/shared/lib/api";
import { GET_REQUEST_TIMEOUT_MS } from "@/shared/lib/polling";

export type TrainingJobKind = "generate" | "lfm-train" | "lfm-eval" | "wake-train";

export interface TrainingProgress {
  step: number;
  total: number | null;
  loss: number | null;
  eta_s: number | null;
}

export interface TrainingRun {
  kind: string;
  started: string;
  running: boolean;
  progress: TrainingProgress | null;
  log_tail: string[];
}

export interface TrainingGeneration {
  rows: number | null;
  target: number | null;
  running: boolean;
}

export type TrainingProfileDetails = Record<
  string,
  { label?: string; model?: string; serving?: string }
>;

/** GET /api/training/status */
export interface TrainingStatus {
  ok: true;
  state: string;
  runtime_up: boolean | null;
  active_profile: string | null;
  run: TrainingRun | null;
  generation: TrainingGeneration | null;
  profile_details: TrainingProfileDetails | null;
}

/** GET /api/training/models */
export interface TrainingModels {
  ok: true;
  profiles: string[];
  active_profile: string | null;
  running: boolean;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function optionalStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is string => typeof entry === "string");
}

function parseProfileDetails(value: unknown): TrainingProfileDetails | null {
  if (!isRecord(value)) return null;
  const details: TrainingProfileDetails = {};
  for (const [name, entry] of Object.entries(value)) {
    if (!isRecord(entry)) continue;
    details[name] = {
      ...(typeof entry.label === "string" ? { label: entry.label } : {}),
      ...(typeof entry.model === "string" ? { model: entry.model } : {}),
      ...(typeof entry.serving === "string" ? { serving: entry.serving } : {}),
    };
  }
  return details;
}

function parseStatus(payload: unknown): TrainingStatus {
  if (!isRecord(payload) || payload.ok !== true || typeof payload.state !== "string") {
    throw new ApiError("incompatible", "invalid training status payload");
  }
  const run = isRecord(payload.run) ? payload.run : null;
  const generation = isRecord(payload.generation) ? payload.generation : null;
  const progress = run && isRecord(run.progress) ? run.progress : null;

  return {
    ok: true,
    state: payload.state,
    runtime_up: typeof payload.runtime_up === "boolean" ? payload.runtime_up : null,
    active_profile:
      typeof payload.active_profile === "string" ? payload.active_profile : null,
    run: run
      ? {
          kind: typeof run.kind === "string" ? run.kind : "",
          started: typeof run.started === "string" ? run.started : "",
          running: Boolean(run.running),
          progress: progress
            ? {
                step: typeof progress.step === "number" ? progress.step : 0,
                total: typeof progress.total === "number" ? progress.total : null,
                loss: typeof progress.loss === "number" ? progress.loss : null,
                eta_s: typeof progress.eta_s === "number" ? progress.eta_s : null,
              }
            : null,
          log_tail: optionalStringArray(run.log_tail),
        }
      : null,
    generation: generation
      ? {
          rows: typeof generation.rows === "number" ? generation.rows : null,
          target: typeof generation.target === "number" ? generation.target : null,
          running: Boolean(generation.running),
        }
      : null,
    profile_details: parseProfileDetails(payload.profile_details),
  };
}

function parseModels(payload: unknown): TrainingModels {
  if (!isRecord(payload) || payload.ok !== true || !Array.isArray(payload.profiles)) {
    throw new ApiError("incompatible", "invalid training models payload");
  }
  return {
    ok: true,
    profiles: optionalStringArray(payload.profiles),
    active_profile:
      typeof payload.active_profile === "string" ? payload.active_profile : null,
    running: Boolean(payload.running),
  };
}

async function protectedFetch(path: string, init: RequestInit): Promise<Response> {
  let response: Response;
  try {
    response = await fetch(path, init);
  } catch (error) {
    throw new ApiError("network", error instanceof Error ? error.message : String(error));
  }
  if (response.status !== 401) return response;

  await renewBrowserSession();
  try {
    const retry = await fetch(path, init);
    if (retry.status === 401) throw new ApiError("stale-session", "HTTP 401", 401);
    return retry;
  } catch (error) {
    if (error instanceof ApiError) throw error;
    throw new ApiError("network", error instanceof Error ? error.message : String(error));
  }
}

async function getTraining(path: string): Promise<unknown> {
  const response = await protectedFetch(path, {
    cache: "no-store",
    credentials: "same-origin",
    signal: AbortSignal.timeout(GET_REQUEST_TIMEOUT_MS),
  });
  const text = await response.text();
  let payload: unknown;
  try {
    payload = text ? JSON.parse(text) : {};
  } catch {
    payload = { error: text };
  }
  if (!response.ok) {
    const message =
      isRecord(payload) && typeof payload.error === "string"
        ? payload.error
        : `HTTP ${response.status}`;
    throw new ApiError(response.status === 409 ? "conflict" : "http", message, response.status);
  }
  return payload;
}

async function postTraining(path: string, body: unknown): Promise<void> {
  const response = await protectedFetch(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    cache: "no-store",
    credentials: "same-origin",
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let payload: unknown;
  try {
    payload = text ? JSON.parse(text) : {};
  } catch {
    payload = { error: text };
  }
  if (!response.ok) {
    const message =
      isRecord(payload) && typeof payload.error === "string"
        ? payload.error
        : `HTTP ${response.status}`;
    throw new ApiError(response.status === 409 ? "conflict" : "http", message, response.status);
  }
}

export async function fetchTrainingStatus(): Promise<TrainingStatus> {
  return parseStatus(await getTraining("/api/training/status"));
}

export async function fetchTrainingModels(): Promise<TrainingModels> {
  return parseModels(await getTraining("/api/training/models"));
}

export async function startTrainingJob(job: TrainingJobKind): Promise<void> {
  await postTraining("/api/training/start", { job, confirm: true });
}

export async function stopTraining(): Promise<void> {
  await postTraining("/api/training/stop", { confirm: true });
}

export async function serveLlama(): Promise<void> {
  await postTraining("/api/training/serve", { confirm: true });
}

export async function switchTrainingProfile(profile: string): Promise<void> {
  await postTraining("/api/training/models/switch", { profile, confirm: true });
}
