import { useState } from "react";
import type { BrowserCockpitView, BrowserTaskDetail } from "../types";
import { formatDuration, relativeTime, statusMeta } from "../state";
import { copyText } from "../diagnostics";
import { visibleTaskActions } from "../taskActions";
import ActionBar from "./ActionBar";
import TaskTerminal from "./TaskTerminal";
import TestInDevPanel from "./TestInDevPanel";

interface Props {
  detail: BrowserTaskDetail;
  onBack?: () => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
  onDismiss?: () => void;
}

export default function TaskDetail({
  detail,
  onBack,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
}: Props) {
  const [metaOpen, setMetaOpen] = useState(false);

  const meta = statusMeta(detail.status);
  const actions = visibleTaskActions(detail.actions);

  const activityLine = (() => {
    const line = detail.agent_activity ?? detail.live_status_summary;
    return line && line !== detail.status_explanation ? line : null;
  })();

  const nowSecs = () => Math.floor(Date.now() / 1000);

  function absoluteTime(unixSecs: number): string | undefined {
    return unixSecs ? new Date(unixSecs * 1000).toLocaleString() : undefined;
  }

  return (
    <div className="task-detail">
      <div className="detail-header" data-mobile-chrome="header">
        <button type="button" className="back" onClick={() => onBack?.()}>
          ← Back
        </button>
        <h1 className="detail-title">{detail.title || detail.qualified_handle}</h1>
        <span className={`interact-pill tone-${meta.tone}`}>{meta.label}</span>
      </div>

      <section className="interact-panel" data-mobile-chrome="actions">
        {detail.runtime_observation_error ? (
          <p className="interact-warning" data-testid="observation-error">
            Observation error: {detail.runtime_observation_error}
          </p>
        ) : null}
        {detail.status_explanation ? (
          <p className="interact-summary">{detail.status_explanation}</p>
        ) : null}
        {activityLine ? (
          <p className="interact-summary interact-activity" data-testid="agent-activity">
            {activityLine}
          </p>
        ) : null}
        {actions.length ? (
          <ActionBar
            actions={actions}
            handle={detail.qualified_handle}
            onCockpit={onCockpit}
            onResult={onResult}
            onMutated={onMutated}
            onDismiss={onDismiss}
          />
        ) : null}
      </section>

      <div>
        <TaskTerminal handle={detail.qualified_handle} />
      </div>

      {detail.repo === "ajax-cli" ? (
        <TestInDevPanel taskHandle={detail.qualified_handle} onResult={onResult} />
      ) : null}

      <details
        className="meta-details"
        open={metaOpen}
        onToggle={(e) => setMetaOpen(e.currentTarget.open)}>
        <summary>Task details</summary>
        <div className="meta-group-label">Branch</div>
        <dl className="detail-grid">
          <dt>Branch</dt>
          <dd className="meta-copy-cell">
            <span className="meta-copy-value">{detail.branch}</span>
            <button type="button" className="meta-copy" onClick={() => copyText(detail.branch)}>
              Copy
            </button>
          </dd>
          <dt>Base</dt>
          <dd>{detail.base_branch}</dd>
          <dt>Worktree</dt>
          <dd className="meta-copy-cell">
            <span className="meta-copy-value">{detail.worktree_path}</span>
            <button type="button" className="meta-copy" onClick={() => copyText(detail.worktree_path)}>
              Copy
            </button>
          </dd>
          {detail.git?.unpushed_commits ? (
            <>
              <dt>Unpushed</dt>
              <dd>{detail.git.unpushed_commits}</dd>
            </>
          ) : null}
        </dl>

        <div className="meta-group-label">Agent</div>
        <dl className="detail-grid">
          <dt>Client</dt>
          <dd>{detail.agent}</dd>
          <dt>Runtime</dt>
          <dd>{detail.agent_status}</dd>
          <dt>Tmux</dt>
          <dd>{detail.tmux_session}</dd>
        </dl>

        <div className="meta-group-label">Activity</div>
        <dl className="detail-grid">
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
            <div className="meta-group-label">Attempts</div>
            <ol className="attempt-list" data-testid="agent-attempts">
              {detail.agent_attempts.map((attempt) => (
                <li key={attempt.started_unix_secs}>
                  <span className="attempt-outcome">{attempt.outcome}</span>
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

        {detail.annotations.length ? (
          <>
            <div className="meta-group-label">Notes</div>
            <ul className="annotation-list" data-testid="task-annotations">
              {detail.annotations.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          </>
        ) : null}
      </details>
    </div>
  );
}
