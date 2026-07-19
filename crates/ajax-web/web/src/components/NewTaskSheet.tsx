import { useEffect, useRef, useState, type FormEvent, type MouseEvent } from "react";
import type { BrowserCockpitView, RepoSummary } from "../types";
import { requestId, startTask } from "../api";
import { startTaskHandle } from "../taskSlug";
import { useSheetDrag } from "../react/useSheetDrag";
import FullscreenLayer from "./FullscreenLayer";
import { Button } from "./ui/button";

interface Props {
  repos: RepoSummary[];
  selectedProject?: string | null;
  onClose?: () => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onOpenTask?: (handle: string) => void;
}

const LAST_AGENT_KEY = "ajax.newTask.agent";
const LAST_REPO_KEY = "ajax.newTask.repo";

const AGENTS = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude" },
  { value: "cursor", label: "Cursor" },
  { value: "opencode", label: "OpenCode" },
] as const;

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

function initialAgent(): string {
  const remembered = readPref(LAST_AGENT_KEY);
  return AGENTS.some((option) => option.value === remembered) ? remembered! : "codex";
}

export default function NewTaskSheet({
  repos,
  selectedProject = null,
  onClose,
  onCockpit,
  onResult,
  onOpenTask,
}: Props) {
  const [repo, setRepo] = useState(() => initialRepo(repos, selectedProject));
  const [title, setTitle] = useState("");
  const [agent, setAgent] = useState(initialAgent);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [dragOffset, setDragOffset] = useState(0);
  const sheetRef = useRef<HTMLDivElement>(null);
  const grabRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    sheetRef.current?.focus();
  }, []);

  useSheetDrag(grabRef, {
    onDismiss: () => onClose?.(),
    onOffset: setDragOffset,
  });

  function savePrefs() {
    try {
      localStorage.setItem(LAST_AGENT_KEY, agent);
      localStorage.setItem(LAST_REPO_KEY, repo);
    } catch {
      // Private mode / storage denied: defaults just won't stick.
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
        agent,
        request_id: requestId(),
      });
      if (result.response.cockpit) onCockpit?.(result.response.cockpit);
      if (!result.ok) {
        const message = result.error?.message ?? "Action failed";
        setError(message);
        onResult?.(message, result.response.output, true);
        return;
      }
      savePrefs();
      onResult?.("Task started", result.response.output, false);
      onOpenTask?.(startTaskHandle(repo, title));
      onClose?.();
    } catch {
      setError("Action failed — network error");
      onResult?.("Could not start task", null, true);
    } finally {
      setSubmitting(false);
    }
  }

  function handleBackdropClick(event: MouseEvent<HTMLDivElement>) {
    if (event.target === event.currentTarget) onClose?.();
  }

  return (
    <FullscreenLayer zIndex={50}>
      <div
        id="new-task-sheet"
        data-testid="new-task-sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby="new-task-title"
        tabIndex={-1}
        ref={sheetRef}
        onClick={handleBackdropClick}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose?.();
        }}
      >
        <form
          className={`sheet-card${dragOffset > 0 ? " is-dragging" : ""}`}
          autoComplete="off"
          onSubmit={submit}
          style={{ transform: `translateY(${dragOffset}px)` }}
        >
          <div className="sheet-grab" aria-hidden="true" ref={grabRef}>
            <span className="sheet-grabber" />
          </div>
          <h2 id="new-task-title">New task</h2>

          <label htmlFor="new-task-repo">Repository</label>
          {repos.length ? (
            <select id="new-task-repo" value={repo} onChange={(e) => setRepo(e.target.value)}>
              {repos.map((option) => (
                <option key={option.name} value={option.name}>
                  {option.name}
                </option>
              ))}
            </select>
          ) : (
            <select id="new-task-repo" disabled>
              <option value="">No repositories configured</option>
            </select>
          )}

          <label htmlFor="new-task-title-input">Title</label>
          <input
            id="new-task-title-input"
            type="text"
            maxLength={80}
            enterKeyHint="go"
            placeholder="Short title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />

          <span className="field-label" id="new-task-agent">
            Agent
          </span>
          <div className="agent-picker" role="radiogroup" aria-labelledby="new-task-agent">
            {AGENTS.map((option) => (
              <button
                key={option.value}
                type="button"
                className={`agent-option${agent === option.value ? " is-selected" : ""}`}
                role="radio"
                aria-checked={agent === option.value}
                onClick={() => setAgent(option.value)}
              >
                {option.label}
              </button>
            ))}
          </div>

          {error ? <p className="sheet-error">{error}</p> : null}

          <div className="sheet-actions">
            <Button type="button" variant="secondary" onClick={() => onClose?.()}>
              Cancel
            </Button>
            <Button type="submit" variant="default" disabled={submitting}>
              Start
            </Button>
          </div>
        </form>
      </div>
    </FullscreenLayer>
  );
}
