import { useState } from "react";
import { requestId, startTask } from "@/shared/lib/api";
import { startTaskHandle } from "@/features/task/taskSlug";
import type { RepoSummary } from "@/shared/lib/types";
import { Button } from "@/shared/ui/button";

interface Props {
  repos: RepoSummary[];
  selectedProject?: string | null;
  onStarted: (handle: string) => void;
}

export default function SessionStarter({ repos, selectedProject, onStarted }: Props) {
  const [repo, setRepo] = useState(selectedProject ?? repos[0]?.name ?? "");
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmedTitle = title.trim();
    if (!repo || !trimmedTitle || busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await startTask({
        repo,
        title: trimmedTitle,
        agent: "cursor",
        request_id: requestId(),
        orchestration_chat: true,
      });
      if (!result.ok) {
        setError(result.error?.message ?? "Failed to start session");
        return;
      }
      onStarted(startTaskHandle(repo, trimmedTitle));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Failed to start session");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section data-testid="session-starter" className="session-starter" aria-labelledby="session-starter-heading">
      <h2 id="session-starter-heading">New session</h2>
      <p className="session-starter-lead">Start a Cursor orchestration chat session.</p>
      <form aria-label="New session" onSubmit={(event) => void handleSubmit(event)}>
        <label htmlFor="session-starter-repo">Repo</label>
        <select
          id="session-starter-repo"
          value={repo}
          onChange={(event) => setRepo(event.target.value)}
        >
          {repos.map((entry) => (
            <option key={entry.name} value={entry.name}>
              {entry.name}
            </option>
          ))}
        </select>
        <label htmlFor="session-starter-title">Title</label>
        <input
          id="session-starter-title"
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder="What should the agent do?"
        />
        {error ? <p className="session-starter-error">{error}</p> : null}
        <Button type="submit" disabled={busy || !title.trim()}>
          Start
        </Button>
      </form>
    </section>
  );
}