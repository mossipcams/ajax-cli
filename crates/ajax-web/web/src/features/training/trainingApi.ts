import {
  AJAX_PROTECTED_HEADERS,
  ApiError,
  fetchProtectedWithSessionRenewal as protectedFetch,
} from "@/shared/lib/api";
import { GET_REQUEST_TIMEOUT_MS } from "@/shared/lib/polling";

export type TrainingJobKind =
  | "generate"
  | "lfm-train"
  | "lfm-eval"
  | "wake-train"
  | "validate"
  | "canary"
  | "promote-dataset"
  | "promote-model"
  | "select-checkpoint";

export const TRAINING_JOB_LABELS: Record<TrainingJobKind, string> = {
  generate: "Data generation",
  "lfm-train": "Unsloth train",
  "lfm-eval": "Unsloth eval",
  "wake-train": "Wake train",
  validate: "Dataset validation",
  canary: "Canary",
  "promote-dataset": "Promote dataset",
  "promote-model": "Promote model",
  "select-checkpoint": "Evaluate checkpoints",
};

export type TrainingDevice = "gpu" | "cpu";

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
  external: boolean;
  progress: TrainingProgress | null;
  log_tail: string[];
}

export interface TrainingGeneration {
  rows: number | null;
  target: number | null;
  phase: string | null;
  running: boolean;
}

export type TrainingProfileDetails = Record<
  string,
  { label?: string; model?: string; serving?: string }
>;

export interface TrainingPipeline {
  run_id: string;
  validated: boolean;
  canary_passed: boolean;
  dataset_promoted: boolean;
  trained: boolean;
  final_eval_passed: boolean;
  promoted_run_id: string | null;
  promoted_record_present: boolean;
}

export interface TrainingStatus {
  ok: true;
  state: string;
  runtime_up: boolean | null;
  active_profile: string | null;
  profiles: string[];
  run: TrainingRun | null;
  generation: TrainingGeneration | null;
  profile_details: TrainingProfileDetails | null;
  device: TrainingDevice | null;
  pipeline: TrainingPipeline | null;
  checkpoint_eval: boolean;
  eval_progress: TrainingEvalProgress[];
}

export interface TrainingEvalProgress {
  checkpoint_label: string;
  group: string;
  labels: string[];
  done: string[];
  current: string | null;
  elapsed_s: number | null;
}

export interface TrainingDataset {
  run_id: string;
  current: boolean;
  promoted: boolean;
  size_bytes: number | null;
  validated: boolean;
  canary_passed: boolean;
  dataset_promoted: boolean;
  trained: boolean;
  final_eval_passed: boolean;
}

export interface TrainingDatasetsResponse {
  datasets: TrainingDataset[];
  promoted_run_id: string | null;
  promoted_record_present: boolean;
}

export type TrainingEvalKind = "checkpoint" | "final";

export interface TrainingEval {
  id: string;
  group: string;
  checkpoint_label: string | null;
  kind: TrainingEvalKind;
  suite: string | null;
  gate_label: string | null;
  created_at: string | null;
  total: number | null;
  passed: number | null;
  pass_rate: number | null;
  promotion_passed: boolean | null;
  failures: string[];
  category_pass_rate: Record<string, number>;
}

export interface TrainingChampion {
  run_id: string | null;
  checkpoint: string | null;
  pass_rates: Record<string, number>;
}

export interface TrainingEvals {
  evals: TrainingEval[];
  champion: TrainingChampion | null;
}

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

function optionalString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function optionalNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function numberRecord(value: unknown): Record<string, number> {
  if (!isRecord(value)) return {};
  const record: Record<string, number> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry === "number" && Number.isFinite(entry)) record[key] = entry;
  }
  return record;
}

function parseDevice(value: unknown): TrainingDevice | null {
  return value === "gpu" || value === "cpu" ? value : null;
}

function parsePipeline(value: unknown): TrainingPipeline | null {
  if (!isRecord(value) || typeof value.run_id !== "string") return null;
  return {
    run_id: value.run_id,
    validated: value.validated === true,
    canary_passed: value.canary_passed === true,
    dataset_promoted: value.dataset_promoted === true,
    trained: value.trained === true,
    final_eval_passed: value.final_eval_passed === true,
    promoted_run_id: optionalString(value.promoted_run_id),
    promoted_record_present: value.promoted_record_present === true,
  };
}

function parseDataset(entry: unknown): TrainingDataset | null {
  if (!isRecord(entry) || typeof entry.run_id !== "string") return null;
  return {
    run_id: entry.run_id,
    current: entry.current === true,
    promoted: entry.promoted === true,
    size_bytes: optionalNumber(entry.size_bytes),
    validated: entry.validated === true,
    canary_passed: entry.canary_passed === true,
    dataset_promoted: entry.dataset_promoted === true,
    trained: entry.trained === true,
    final_eval_passed: entry.final_eval_passed === true,
  };
}

function parseDatasets(payload: unknown): TrainingDatasetsResponse {
  if (!isRecord(payload) || !Array.isArray(payload.datasets)) {
    throw new ApiError("incompatible", "invalid training datasets payload");
  }
  return {
    datasets: payload.datasets
      .map(parseDataset)
      .filter((entry): entry is TrainingDataset => entry !== null),
    promoted_run_id: optionalString(payload.promoted_run_id),
    promoted_record_present: payload.promoted_record_present === true,
  };
}

function parseEval(entry: unknown): TrainingEval | null {
  if (!isRecord(entry)) return null;
  const id =
    typeof entry.id === "string" ? entry.id : typeof entry.id === "number" ? String(entry.id) : null;
  if (id === null || (entry.kind !== "checkpoint" && entry.kind !== "final")) return null;
  return {
    id,
    group: optionalString(entry.group) ?? "",
    checkpoint_label: optionalString(entry.checkpoint_label),
    kind: entry.kind,
    suite: optionalString(entry.suite),
    gate_label: optionalString(entry.gate_label),
    created_at: optionalString(entry.created_at),
    total: optionalNumber(entry.total),
    passed: optionalNumber(entry.passed),
    pass_rate: optionalNumber(entry.pass_rate),
    promotion_passed: typeof entry.promotion_passed === "boolean" ? entry.promotion_passed : null,
    failures: optionalStringArray(entry.failures),
    category_pass_rate: numberRecord(entry.category_pass_rate),
  };
}

function parseChampion(value: unknown): TrainingChampion | null {
  if (!isRecord(value)) return null;
  return {
    run_id: optionalString(value.run_id),
    checkpoint: optionalString(value.checkpoint),
    pass_rates: numberRecord(value.pass_rates),
  };
}

function parseEvals(payload: unknown): TrainingEvals {
  if (!isRecord(payload) || !Array.isArray(payload.evals)) {
    throw new ApiError("incompatible", "invalid training evals payload");
  }
  return {
    evals: payload.evals
      .map(parseEval)
      .filter((entry): entry is TrainingEval => entry !== null),
    champion: parseChampion(payload.champion),
  };
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
    profiles: optionalStringArray(payload.profiles),
    run: run
      ? {
          kind: typeof run.kind === "string" ? run.kind : "",
          started: typeof run.started === "string" ? run.started : "",
          running: Boolean(run.running),
          external: run.external === true,
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
          phase: typeof generation.phase === "string" ? generation.phase : null,
          running: Boolean(generation.running),
        }
      : null,
    profile_details: parseProfileDetails(payload.profile_details),
    device: parseDevice(payload.device),
    pipeline: parsePipeline(payload.pipeline),
    checkpoint_eval: payload.checkpoint_eval === true,
    eval_progress: Array.isArray(payload.eval_progress)
      ? payload.eval_progress.filter(isEvalProgress)
      : [],
  };
}

function isEvalProgress(value: unknown): value is TrainingEvalProgress {
  return isRecord(value) &&
    typeof value.checkpoint_label === "string" && typeof value.group === "string" &&
    Array.isArray(value.labels) && value.labels.length > 0 &&
    value.labels.every((label) => typeof label === "string") &&
    Array.isArray(value.done) && value.done.every((label) => typeof label === "string") &&
    (value.current === null || typeof value.current === "string") &&
    (value.elapsed_s === null || (typeof value.elapsed_s === "number" &&
      Number.isFinite(value.elapsed_s) && value.elapsed_s >= 0));
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

async function getTraining(path: string): Promise<unknown> {
  const response = await protectedFetch(path, {
    cache: "no-store",
    credentials: "same-origin",
    headers: { ...AJAX_PROTECTED_HEADERS },
    signal: AbortSignal.timeout(GET_REQUEST_TIMEOUT_MS),
  });
  const text = await response.text();
  let payload: unknown;
  try {
    payload = text ? JSON.parse(text) : {};
  } catch {
    // Non-JSON bodies (e.g. a proxy's HTML 502 page) are never surfaced as the message.
    payload = undefined;
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
    headers: { ...AJAX_PROTECTED_HEADERS, "content-type": "application/json" },
    cache: "no-store",
    credentials: "same-origin",
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let payload: unknown;
  try {
    payload = text ? JSON.parse(text) : {};
  } catch {
    // Non-JSON bodies (e.g. a proxy's HTML 502 page) are never surfaced as the message.
    payload = undefined;
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

export async function fetchTrainingDatasets(): Promise<TrainingDatasetsResponse> {
  return parseDatasets(await getTraining("/api/training/datasets"));
}

export async function fetchTrainingEvals(): Promise<TrainingEvals> {
  return parseEvals(await getTraining("/api/training/evals"));
}

export async function selectTrainingDataset(runId: string): Promise<void> {
  await postTraining("/api/training/datasets/select", { run_id: runId, confirm: true });
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
