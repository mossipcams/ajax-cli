import type { RepoSummary } from "@/shared/lib/types";

// Repositories as an entity view, not a filter row. Every number here is a
// count Rust already computed on `RepoSummary`; the browser derives none of them
// and shows nothing when the server omits one.

interface Props {
  repos: RepoSummary[];
  selectedProject?: string | null;
  onSelectProject?: (project: string) => void;
}

/** Count chips in escalation order. A zero is not news — only non-zero renders,
 * so a healthy repo reads as one quiet line. */
const COUNTS: Array<{ field: keyof RepoSummary; label: (n: number) => string; tone: string }> = [
  { field: "attention_items", label: (n) => `${n} need you`, tone: "attention" },
  { field: "reviewable_tasks", label: (n) => `${n} to review`, tone: "ready" },
  { field: "active_tasks", label: (n) => `${n} active`, tone: "running" },
  { field: "cleanable_tasks", label: (n) => `${n} to clean up`, tone: "muted" },
];

function count(repo: RepoSummary, field: keyof RepoSummary): number {
  const value = repo[field];
  return typeof value === "number" && value > 0 ? value : 0;
}

export default function RepoPanel({ repos, selectedProject = null, onSelectProject }: Props) {
  if (repos.length === 0) return null;

  return (
    <section className="repo-panel" aria-label="Repositories">
      <div className="task-band-title">
        <span className="task-band-label">Repositories</span>
        <span className="task-band-count">{repos.length}</span>
      </div>
      <div className="repo-list">
        {repos.map((repo) => {
          const chips = COUNTS.filter((chip) => count(repo, chip.field) > 0);
          return (
            <button
              key={repo.name}
              type="button"
              className={`repo-row${selectedProject === repo.name ? " is-active" : ""}`}
              data-repo={repo.name}
              aria-current={selectedProject === repo.name ? "true" : undefined}
              onClick={() => onSelectProject?.(repo.name)}
            >
              <span className="repo-row-main">
                <span className="repo-row-name">{repo.name}</span>
                {/* The isolate is load-bearing: the wrapper is `direction: rtl`
                    so overflow clips the *start* of the path, and without it the
                    slashes render reversed. */}
                {repo.path ? (
                  <span className="repo-row-path">
                    <bdi>{repo.path}</bdi>
                  </span>
                ) : null}
              </span>
              {chips.length > 0 ? (
                <span className="repo-row-counts" data-testid={`repo-counts-${repo.name}`}>
                  {chips.map((chip) => (
                    <span key={String(chip.field)} className={`repo-chip tone-${chip.tone}`}>
                      {chip.label(count(repo, chip.field))}
                    </span>
                  ))}
                </span>
              ) : (
                <span className="repo-row-counts repo-row-clear">clear</span>
              )}
            </button>
          );
        })}
      </div>
    </section>
  );
}
