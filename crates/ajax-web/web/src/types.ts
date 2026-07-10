// Browser-facing DTOs. These mirror the Rust serialization in
// `crates/ajax-web/src/slices/*` exactly. The browser must not derive
// lifecycle, action validity, or status from these; it renders them.

/** Canonical four-state task status owned by Rust. */
export type TaskStatus = "running" | "waiting" | "idle" | "error";

/** Connection display states surfaced in the UI. */
export type ConnectionState =
  | "connected"
  | "checking"
  | "reconnecting"
  | "disconnected"
  | "backend unreachable"
  | "stale session";

/** Hash-route kinds. */
export type RouteKind = "dashboard" | "project" | "task" | "settings";

export interface WebAction {
  action: string;
  label: string;
  destructive: boolean;
  confirmation_required: boolean;
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
}
