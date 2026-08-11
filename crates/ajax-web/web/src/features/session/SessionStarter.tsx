import { useState, type FormEvent } from "react";
import type { BrowserCockpitView, RepoSummary } from "@/shared/lib/types";
import { requestId, startTask } from "@/shared/lib/api";
import { startTaskHandle } from "@/features/task/taskSlug";
import { Button } from "@/shared/ui/button";
import SessionModelSelect from "./SessionModelSelect";
import { useSessionModelPreference } from "./sessionModel";

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
  title: string;
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
  const [model, setModel] = useSessionModelPreference();

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
      const normalizedTitle = title.trim();
      const result = await startTask({
        repo,
        title: normalizedTitle,
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
      onStarted?.(startTaskHandle(repo, normalizedTitle), {
        title: normalizedTitle,
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
      <header className="session-header">
        {onBack ? (
          <button type="button" className="session-header-back" onClick={onBack}>
            ← Back
          </button>
        ) : null}
        <h1 className="session-title">New session</h1>
        <span className="session-status-pill tone-muted" data-testid="session-agent-lock">
          Cursor
        </span>
      </header>

      <form className="session-starter-form" aria-label="Start session" onSubmit={submit}>
        <div className="session-field">
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
        </div>

        <SessionModelSelect id="session-model" value={model} onChange={setModel} />

        <div className="session-field">
          <label htmlFor="session-title">Title</label>
          <input
            id="session-title"
            type="text"
            maxLength={80}
            enterKeyHint="next"
            placeholder="What should the agent do?"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </div>

        <div className="session-starter-brief">
          <p className="session-starter-brief-label">Brief — optional</p>

          <div className="session-field">
            <label htmlFor="session-constraints">Constraints</label>
            <textarea
              id="session-constraints"
              rows={2}
              placeholder="Boundaries, must-nots, style"
              value={constraints}
              onChange={(e) => setConstraints(e.target.value)}
            />
          </div>

          <div className="session-field">
            <label htmlFor="session-outcome">Expected outcome</label>
            <textarea
              id="session-outcome"
              rows={2}
              placeholder="What done looks like"
              value={expectedOutcome}
              onChange={(e) => setExpectedOutcome(e.target.value)}
            />
          </div>
        </div>

        {error ? <p className="session-error">{error}</p> : null}

        <div className="session-starter-actions">
          <Button type="submit" variant="default" disabled={submitting}>
            {submitting ? "Starting…" : "Start session"}
          </Button>
        </div>
      </form>
    </section>
  );
}
