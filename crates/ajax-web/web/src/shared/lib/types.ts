// Browser-facing DTOs. These mirror the Rust serialization in
// `crates/ajax-web/src/slices/*` exactly. The browser must not derive
// lifecycle, action validity, or status from these; it renders them.

import type { ApiError } from "./api";

/** Canonical task status owned by Rust (`TaskStatus` serde lowercase). */
export type TaskStatus = "running" | "waiting" | "idle" | "error" | "unknown";

/** Connection display states surfaced in the UI. */
export type ConnectionState =
  | "connected"
  | "checking"
  | "reconnecting"
  | "disconnected"
  | "backend unreachable"
  | "stale session";

/** Hash-route kinds. */
export type RouteKind = "dashboard" | "project" | "task" | "diff" | "settings";

/** Read-only Diff Review projection (mirrors ajax-web `diff_review` DTOs). */
export interface PullRequestView {
  number: number;
  title: string;
  url: string;
  state: string;
  head_ref: string;
  head_sha: string | null;
}

export interface DiffHunkView {
  header: string;
  lines: string[];
}

export type DiffFileRole = "signal" | "noise";

export interface DiffFileView {
  path: string;
  status: string;
  additions: number;
  deletions: number;
  role: DiffFileRole;
  hunks: DiffHunkView[];
}

export interface DiffTotalsView {
  files: number;
  signal: number;
  noise: number;
  additions: number;
  deletions: number;
}

export type DiffFlagKind =
  | "unexpected_path"
  | "deleted_test"
  | "secret_pattern"
  | "permission_widen"
  | "dependency_manifest"
  | "deleted_check_path";

export type DiffFlagSeverity = "info" | "warn" | "critical";

export interface DiffFlagView {
  kind: DiffFlagKind;
  severity: DiffFlagSeverity;
  path: string;
}

export interface DiffJudgmentView {
  totals: DiffTotalsView;
  reading_order: string[];
  flags: DiffFlagView[];
}

export interface TaskDiffView {
  source: string;
  pr: PullRequestView | null;
  files: DiffFileView[];
  fell_back_from_pr?: number | null;
  judgment: DiffJudgmentView;
}

export interface BranchAdoptionPlan {
  expected_branch: string;
  observed_branch: string;
}

export interface WebAction {
  action: string;
  label: string;
  destructive: boolean;
  confirmation_required: boolean;
  branch_adoption?: BranchAdoptionPlan;
}

export interface RepoSummary {
  name: string;
  attention_items?: number;
  [key: string]: unknown;
}

export interface ReposResponse {
  repos: RepoSummary[];
}

export interface AnnotationItem {
  task_handle: string;
  severity: number;
  [key: string]: unknown;
}

export interface InboxResponse {
  items: AnnotationItem[];
}

export interface BrowserBackend {
  authority: string;
  control_enabled: boolean;
  warning?: string | null;
}

export interface BrowserTaskCard {
  id: string;
  qualified_handle: string;
  repo: string;
  title: string;
  status: TaskStatus;
  status_explanation?: string | null;
  last_activity_unix_secs: number;
  actions: WebAction[];
}

export interface BrowserCockpitView {
  backend: BrowserBackend;
  repos: ReposResponse;
  cards: BrowserTaskCard[];
  inbox: InboxResponse;
}

export interface GitStatus {
  unpushed_commits?: number;
  [key: string]: unknown;
}

export interface TmuxStatus {
  [key: string]: unknown;
}

export interface BrowserAgentAttempt {
  started_unix_secs: number;
  completed_unix_secs?: number | null;
  outcome: string;
}

export interface BrowserTaskDetail {
  qualified_handle: string;
  repo: string;
  title: string;
  branch: string;
  base_branch: string;
  worktree_path: string;
  tmux_session: string;
  lifecycle: string;
  agent: string;
  agent_status: string;
  status: TaskStatus;
  status_explanation?: string | null;
  runtime_observation_error?: string | null;
  actions: WebAction[];
  live_status_kind?: string | null;
  live_status_summary?: string | null;
  agent_activity?: string | null;
  git?: GitStatus | null;
  tmux?: TmuxStatus | null;
  annotations: string[];
  created_unix_secs: number;
  last_activity_unix_secs: number;
  agent_attempts: BrowserAgentAttempt[];
}

export interface StartTaskRequest {
  repo: string;
  title: string;
  agent: string;
  request_id: string;
}

export interface OperationRequest {
  task_handle: string;
  action: string;
  request_id: string;
  confirmed?: boolean;
  branch_adoption?: BranchAdoptionPlan;
}

/** Operation/start envelopes return a refreshed projection on state change. */
export interface OperationResponse {
  ok?: boolean;
  request_id?: string;
  state_changed?: boolean;
  cockpit?: BrowserCockpitView;
  output?: string | null;
  error?: string | null;
  restarting?: boolean;
}

export interface VersionResponse {
  version: string;
  test_in_stable?: boolean;
}

export interface PushVapidResponse {
  public_key: string;
}

export interface PushTestSubscription {
  endpoint: string;
  keys: {
    p256dh: string;
    auth: string;
  };
  /** Server waits this long before delivering so the PWA can be fully quit. */
  delay_ms?: number;
}

export type DevDeployPhase =
  | "ready_to_deploy"
  | "building"
  | "restarting"
  | "dev_ready"
  | "failed";

export interface DevSlotOccupant {
  task_handle: string;
  title: string;
  branch: string;
  commit_sha: string;
  dirty: boolean;
  deployed_at_unix_secs: number;
}

export interface DevDeployStatus {
  phase: DevDeployPhase;
  phase_label: string;
  shared_slot: boolean;
  active: boolean;
  error?: string | null;
  occupant?: DevSlotOccupant | null;
}

export interface DevDeployResponse {
  ok: boolean;
  deploy: DevDeployStatus;
  message?: string;
  error?: string;
}

/** Ajax Web Session symbol hit (mirrors ajax-web `web_session` wire DTOs). */
export type WebSessionSymbolKind =
  | "function"
  | "method"
  | "struct"
  | "class"
  | "type"
  | "interface"
  | "file";

export interface WebSessionSymbolContext {
  id: string;
  name: string;
  kind: WebSessionSymbolKind;
  path: string;
  startLine: number;
  endLine: number;
  preview: string;
  source: string;
}

export type RemoteResource<T> =
  | { status: "loading"; data: null; error: null }
  | { status: "ready"; data: T; error: null }
  | { status: "stale"; data: T; error: ApiError }
  | { status: "error"; data: null; error: ApiError };
