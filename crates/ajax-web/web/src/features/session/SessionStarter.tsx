import { useState, type FormEvent } from "react";
import type { BrowserCockpitView, RepoSummary } from "@/shared/lib/types";
import { requestId, startTask } from "@/shared/lib/api";
import { startTaskHandle } from "@/features/task/taskSlug";
import { Button } from "@/shared/ui/button";

const LAST_REPO_KEY = "ajax.newTask.repo";
const CURSOR_AGENT = "cursor";

function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function initialRepo(repos: RepoSummary[], selectedProject: string | null): string {
  if (selectedProject && repos.some((r) => r.name === selectedProject)) return selectedProject;
  const remembered = readPref(LAST_REPO_KEY);
  if (remembered && repos.some((r) => r.name === remembered)) return remembered;
  return repos[0]?.name ?? "";
}

export interface SessionStarterContext {
  constraints: string;
  expectedOutcome: string;
}

interface Props {
  repos: RepoSummary[];
  selectedProject?: string | null;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onStarted?: (handle: string, starter: SessionStarterContext) => void;
  onBack?: () => void;
}

export default function SessionStarter({
  repos,
  selectedProject = null,
  onCockpit,
  onStarted,
  onBack,
}: Props) {
  const [repo, setRepo] = useState(() => initialRepo(repos, selectedProject));
  const [title, setTitle] = useState("");
  const [constraints, setConstraints] = useState("");
  const [expectedOutcome, setExpectedOutcome] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  function savePrefs() {
    try {
      localStorage.setItem(LAST_REPO_KEY, repo);
    } catch {
      // Storage may be unavailable.
    }
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!repo) {
      setError("Pick a repository first");
      return;
    }
    if (!title.trim()) {
      setError("Add a title");
      return;
    }
    setError(null);
    setSubmitting(true);
    try {
      const result = await startTask({
        repo,
        title: title.trim(),
        agent: CURSOR_AGENT,
        orchestration_chat: true,
        request_id: requestId(),
      });
      if (result.response.cockpit) onCockpit?.(result.response.cockpit);
      if (!result.ok) {
        setError(result.error?.message ?? "Action failed");
        return;
      }
      savePrefs();
      onStarted?.(startTaskHandle(repo, title), {
        constraints: constraints.trim(),
        expectedOutcome: expectedOutcome.trim(),
      });
    } catch {
      setError("Action failed — network error");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section className="session-page session-starter" data-testid="session-starter">
      <div className="session-header">
        {onBack ? (
          <button type="button" className="back" onClick={onBack}>
            ← Back
          </button>
        ) : null}
        <h1 className="session-title">New session</h1>
      </div>

      <form className="session-starter-form" aria-label="Start session" onSubmit={submit}>
        <label htmlFor="session-repo">Repository</label>
        {repos.length ? (
          <select id="session-repo" value={repo} onChange={(e) => setRepo(e.target.value)}>
            {repos.map((option) => (
              <option key={option.name} value={option.name}>
                {option.name}
              </option>
            ))}
          </select>
        ) : (
          <select id="session-repo" disabled>
            <option value="">No repositories configured</option>
          </select>
        )}

        <label htmlFor="session-title">Title</label>
        <input
          id="session-title"
          type="text"
          maxLength={80}
          enterKeyHint="next"
          placeholder="Short title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />

        <label htmlFor="session-constraints">Constraints</label>
        <textarea
          id="session-constraints"
          rows={3}
          placeholder="Boundaries, must-nots, style"
          value={constraints}
          onChange={(e) => setConstraints(e.target.value)}
        />

        <label htmlFor="session-outcome">Expected outcome</label>
        <textarea
          id="session-outcome"
          rows={3}
          placeholder="What done looks like"
          value={expectedOutcome}
          onChange={(e) => setExpectedOutcome(e.target.value)}
        />

        <p className="session-agent-lock" data-testid="session-agent-lock">
          Agent: Cursor (ACP orchestration chat)
        </p>

        {error ? <p className="session-error">{error}</p> : null}

        <div className="session-starter-actions">
          <Button type="submit" variant="default" disabled={submitting}>
            Start session
          </Button>
        </div>
      </form>
    </section>
  );
}
