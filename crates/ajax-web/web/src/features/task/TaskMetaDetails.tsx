import type { BrowserTaskDetail } from "@/shared/lib/types";
import { formatDuration, relativeTime } from "@/shared/lib/state";
import { copyText } from "@/shared/lib/clipboard";
import TestInDevPanel from "./TestInDevPanel";

interface Props {
  detail: BrowserTaskDetail;
  embedded?: boolean;
  /** When the sheet identity header already shows branch. */
  hideBranch?: boolean;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
}

const ANNOTATION_KIND_LABELS: Record<string, string> = {
  NeedsMe: "needs you",
  Broken: "broken",
  Reviewable: "reviewable",
  Cleanable: "cleanable",
};

const EVIDENCE_LABELS: Record<string, Record<string, string> | string> = {
  LiveStatus: {
    WaitingForApproval: "waiting for approval",
    WaitingForInput: "waiting for input",
    AuthRequired: "auth required",
    RateLimited: "rate limited",
    ContextLimit: "context limit",
    CommandFailed: "command failed",
    Blocked: "blocked",
    WorktreeMissing: "worktree missing",
    TmuxMissing: "tmux missing",
    TaskWindowMissing: "task window missing",
    MergeConflict: "merge conflict",
    Done: "done",
    ShellIdle: "shell idle",
    CommandRunning: "command running",
    TestsRunning: "tests running",
    AgentRunning: "agent running",
    CiFailed: "ci failed",
    CiPending: "ci running",
    Unknown: "live status",
  },
  AgentStatus: {
    NotStarted: "agent not started",
    Running: "agent running",
    Waiting: "agent waiting",
    Blocked: "agent blocked",
    Done: "agent done",
    Dead: "agent dead",
    Unknown: "agent status not observed",
  },
  SideFlag: {
    Dirty: "dirty",
    AgentRunning: "agent running",
    AgentDead: "agent dead",
    NeedsInput: "needs input",
    TestsFailed: "tests failed",
    TmuxMissing: "tmux missing",
    WorktreeMissing: "worktree missing",
    TaskWindowMissing: "task window missing",
    BranchMissing: "branch missing",
    Stale: "stale",
    Conflicted: "conflicted",
    Unpushed: "unpushed",
  },
  Lifecycle: {
    Created: "created",
    Provisioning: "provisioning",
    Active: "active",
    Waiting: "waiting",
    Reviewable: "reviewable",
    Mergeable: "mergeable",
    Merged: "merged",
    Cleanable: "cleanable",
    Removing: "removing",
    TeardownIncomplete: "teardown incomplete",
    Removed: "removed",
    Orphaned: "orphaned",
    Error: "error",
  },
  Substrate: {
    WorktreeMissing: "worktree missing",
    TmuxMissing: "tmux missing",
    TaskWindowMissing: "task window missing",
    BranchMissing: "branch missing",
  },
  RuntimeObservationFailed: "runtime observation failed",
  CheckoutMismatch: "checkout mismatch",
};

const DEBUG_ANNOTATION =
  /^Annotation\s*\{\s*kind:\s*(\w+),\s*severity:\s*\d+,\s*evidence:\s*(\w+)(?:\(([^)]*)\))?,\s*suggests:\s*\w+\s*\}$/;

function evidenceLabel(type: string, variant?: string): string {
  const entry = EVIDENCE_LABELS[type];
  if (typeof entry === "string") return entry;
  if (variant && entry?.[variant]) return entry[variant];
  if (variant) return variant.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();
  return type.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();
}

/** Mirrors ajax-core `Annotation::row_label` for Debug strings from the web API. */
export function humanizeTaskAnnotation(note: string): string | null {
  const trimmed = note.trim();
  if (!trimmed) return null;
  const debug = DEBUG_ANNOTATION.exec(trimmed);
  if (!debug) return trimmed;

  const [, kind, evidenceType, evidenceVariant] = debug;
  const kindLabel = ANNOTATION_KIND_LABELS[kind] ?? kind.toLowerCase();
  const evLabel = evidenceLabel(evidenceType, evidenceVariant);

  if (
    kind === "NeedsMe" &&
    evidenceType === "LiveStatus" &&
    (evidenceVariant === "WaitingForApproval" || evidenceVariant === "WaitingForInput")
  ) {
    return evLabel;
  }
  return `${kindLabel} · ${evLabel}`;
}

function MetaCopyCell({ value }: { value: string }) {
  return (
    <dd className="meta-copy-cell">
      <span className="meta-copy-value" title={value}>
        {value}
      </span>
      <button type="button" className="meta-copy" onClick={() => copyText(value)}>
        Copy
      </button>
    </dd>
  );
}

export default function TaskMetaDetails({
  detail,
  embedded = false,
  hideBranch = false,
  onResult,
}: Props) {
  const nowSecs = () => Math.floor(Date.now() / 1000);

  function absoluteTime(unixSecs: number): string | undefined {
    return unixSecs ? new Date(unixSecs * 1000).toLocaleString() : undefined;
  }

  const humanNotes = detail.annotations
    .map((note) => humanizeTaskAnnotation(note))
    .filter((note): note is string => Boolean(note));

  const body = (
    <div className="meta-details-body" data-testid={embedded ? "task-meta-details-embedded" : undefined}>
      {detail.repo === "ajax-cli" ? (
        <TestInDevPanel taskHandle={detail.qualified_handle} onResult={onResult} />
      ) : null}
      <dl className="detail-grid">
        {hideBranch ? null : (
          <>
            <dt>Branch</dt>
            <MetaCopyCell value={detail.branch} />
          </>
        )}
        <dt>Base</dt>
        <dd>{detail.base_branch}</dd>
        <dt>Worktree</dt>
        <MetaCopyCell value={detail.worktree_path} />
        {detail.git?.unpushed_commits ? (
          <>
            <dt>Unpushed</dt>
            <dd>{detail.git.unpushed_commits}</dd>
          </>
        ) : null}
        <dt>Client</dt>
        <dd>{detail.agent}</dd>
        <dt>Runtime</dt>
        <dd>{detail.agent_status}</dd>
        <dt>Tmux</dt>
        <dd>{detail.tmux_session}</dd>
        <dt>Created</dt>
        <dd title={absoluteTime(detail.created_unix_secs)}>
          {relativeTime(detail.created_unix_secs, nowSecs())}
        </dd>
        <dt>Active</dt>
        <dd title={absoluteTime(detail.last_activity_unix_secs)}>
          {relativeTime(detail.last_activity_unix_secs, nowSecs())}
        </dd>
      </dl>

      {detail.agent_attempts.length ? (
        <>
          <h3 className="meta-list-heading">Attempts</h3>
          <ol className="attempt-list" data-testid="agent-attempts">
            {detail.agent_attempts.map((attempt) => (
              <li key={attempt.started_unix_secs}>
                <span className="attempt-outcome">{attempt.outcome}</span>{" "}
                <span className="attempt-when">
                  {relativeTime(attempt.started_unix_secs, nowSecs())}
                  {" · "}
                  {attempt.completed_unix_secs
                    ? formatDuration(attempt.completed_unix_secs - attempt.started_unix_secs)
                    : "in progress"}
                </span>
              </li>
            ))}
          </ol>
        </>
      ) : null}

      {humanNotes.length ? (
        <>
          <h3 className="meta-list-heading">Notes</h3>
          <ul className="annotation-list" data-testid="task-annotations">
            {humanNotes.map((note) => (
              <li key={note}>{note}</li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );

  if (embedded) {
    return body;
  }

  return (
    <details className="meta-details">
      <summary>Task details</summary>
      {body}
    </details>
  );
}
