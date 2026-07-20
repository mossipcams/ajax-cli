import type { BrowserTaskDetail } from "@/shared/lib/types";
import { formatDuration, relativeTime } from "@/shared/lib/state";
import { copyText } from "@/shared/lib/clipboard";
import TestInDevPanel from "./TestInDevPanel";

interface Props {
  detail: BrowserTaskDetail;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
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

export default function TaskMetaDetails({ detail, onResult }: Props) {
  const nowSecs = () => Math.floor(Date.now() / 1000);

  function absoluteTime(unixSecs: number): string | undefined {
    return unixSecs ? new Date(unixSecs * 1000).toLocaleString() : undefined;
  }

  return (
    <details className="meta-details">
      <summary>Task details</summary>
      <div className="meta-details-body">
        {detail.repo === "ajax-cli" ? (
          <TestInDevPanel taskHandle={detail.qualified_handle} onResult={onResult} />
        ) : null}
        <dl className="detail-grid">
          <dt>Branch</dt>
          <MetaCopyCell value={detail.branch} />
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
            <h3 className="meta-list-heading">Notes</h3>
            <ul className="annotation-list" data-testid="task-annotations">
              {detail.annotations.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          </>
        ) : null}
      </div>
    </details>
  );
}
