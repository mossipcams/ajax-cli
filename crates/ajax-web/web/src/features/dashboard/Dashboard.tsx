import { useEffect, useMemo, useState } from "react";
import type { AttentionBand, BrowserCockpitView, BrowserTaskCard } from "@/shared/lib/types";
import {
  filterByProject,
  formatDuration,
  isQuiet,
  relativeTime,
  reposWithFault,
  sortCards,
  statusMeta,
} from "@/shared/lib/state";
import { visibleTaskActions } from "@/features/task/taskActions";
import ActionBar from "@/features/task/ActionBar";

// The dashboard is a control panel, not a ledger. Its contract: anything Rust says
// a task can safely do is one tap away here, so the terminal stays the exception.
// Nothing lives behind a gesture, and the browser derives no task truth — bands,
// ordering and the action list all arrive from the server.

interface Props {
  cockpit: BrowserCockpitView;
  selectedProject?: string | null;
  onSelectProject?: (project: string | null) => void;
  onOpenTask?: (handle: string) => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
}

type RowDispatch = Pick<Props, "onCockpit" | "onResult" | "onMutated" | "onOpenTask">;

interface TaskRowProps extends RowDispatch {
  card: BrowserTaskCard;
  nowSecs: number;
}

const BANDS: Array<[AttentionBand, string]> = [
  ["needs-you", "Needs you"],
  ["review", "Ready to review"],
  ["active", "Active"],
  ["idle", "Idle"],
];

function TaskRow({ card, nowSecs, onOpenTask, onCockpit, onResult, onMutated }: TaskRowProps) {
  const meta = statusMeta(card.status);
  const quiet = isQuiet(card, nowSecs);
  // Destructive stays on task detail: Drop is never the next step you want a
  // thumb to find while scanning a list.
  const actions = visibleTaskActions(card.actions).filter((action) => !action.destructive);
  const explanation =
    card.status_explanation && card.status_explanation.toLowerCase() !== meta.label.toLowerCase()
      ? card.status_explanation
      : null;

  return (
    <div
      className={`task-row tone-${meta.tone}${quiet ? " is-quiet" : ""}`}
      data-handle={card.qualified_handle}
      data-testid={`task-row-${card.qualified_handle}`}
    >
      <button
        type="button"
        className="task-row-tap"
        onClick={() => onOpenTask?.(card.qualified_handle)}
      >
        <span className={`status-dot tone-${meta.tone}`} aria-hidden="true" />
        <span className="task-row-main">
          <span className="task-row-head">
            <span className="task-row-title">{card.title || card.qualified_handle}</span>
            {card.last_activity_unix_secs ? (
              <span className="task-row-time">
                {relativeTime(card.last_activity_unix_secs, nowSecs)}
              </span>
            ) : null}
          </span>
          <span className="task-row-sub">
            <span className="task-row-handle">{card.qualified_handle}</span>
            {quiet ? (
              <span className="task-row-quiet">
                Quiet {formatDuration(nowSecs - card.last_activity_unix_secs)} — no output
              </span>
            ) : explanation ? (
              <span className="task-row-note">{explanation}</span>
            ) : null}
          </span>
        </span>
      </button>

      {actions.length > 0 ? (
        <div className="task-row-actions" data-testid="task-row-actions">
          <ActionBar
            actions={actions}
            handle={card.qualified_handle}
            onCockpit={onCockpit}
            onResult={onResult}
            onMutated={onMutated}
          />
        </div>
      ) : null}
    </div>
  );
}

export default function Dashboard({
  cockpit,
  selectedProject = null,
  onSelectProject,
  onOpenTask,
  onCockpit,
  onResult,
  onMutated,
}: Props) {
  const [nowSecs, setNowSecs] = useState(() => Math.floor(Date.now() / 1000));
  const [stableOrder, setStableOrder] = useState<string[]>([]);

  // Quiet detection turns on a 4-minute boundary, so the clock must tick faster
  // than the 60s row-time refresh to flip a running row to "quiet" on time.
  useEffect(() => {
    const timer = setInterval(() => setNowSecs(Math.floor(Date.now() / 1000)), 30_000);
    return () => clearInterval(timer);
  }, []);

  const projects = useMemo(
    () =>
      [
        ...new Set([
          ...cockpit.cards.map((card) => card.repo),
          ...(cockpit.repos?.repos ?? []).map((repo) => repo.name),
        ]),
      ].sort(),
    [cockpit.cards, cockpit.repos?.repos],
  );

  // A faulted repo reads on its pill as a dot, not a count — the filter row
  // carries health, not a metric. Faults are counted across the whole fleet so
  // the dot is honest whatever pill is active.
  const faultRepos = useMemo(() => reposWithFault(cockpit.cards), [cockpit.cards]);

  // Rust ranks the cards; the browser only keeps that order stable across polls
  // so rows don't reshuffle under the operator's thumb mid-tap.
  const cards = useMemo(
    () => sortCards(filterByProject(cockpit.cards, selectedProject), stableOrder),
    [cockpit.cards, selectedProject, stableOrder],
  );

  useEffect(() => {
    const next = cards.map((card) => card.qualified_handle);
    setStableOrder((prev) =>
      next.length === prev.length && next.every((handle, i) => handle === prev[i]) ? prev : next,
    );
  }, [cards]);

  const dispatch: RowDispatch = { onOpenTask, onCockpit, onResult, onMutated };

  const rows = (band: AttentionBand) =>
    cards
      .filter((card) => card.attention === band)
      .map((card) => (
        <TaskRow key={card.qualified_handle} card={card} nowSecs={nowSecs} {...dispatch} />
      ));

  return (
    <>
      {projects.length > 0 ? (
        <nav className="project-nav" aria-label="Projects">
          <button
            type="button"
            className={`project-pill${!selectedProject ? " is-active" : ""}`}
            aria-current={!selectedProject ? "true" : undefined}
            onClick={() => onSelectProject?.(null)}
          >
            All
          </button>
          {projects.map((project) => {
            const faulted = faultRepos.has(project);
            return (
              <button
                key={project}
                type="button"
                className={`project-pill${selectedProject === project ? " is-active" : ""}`}
                aria-label={faulted ? `${project} — has a fault` : project}
                aria-current={selectedProject === project ? "true" : undefined}
                onClick={() => onSelectProject?.(project)}
              >
                {project}
                {faulted ? <span className="pill-fault-dot" aria-hidden="true" /> : null}
              </button>
            );
          })}
        </nav>
      ) : null}

      {cards.length > 0 ? (
        <section className="tasks" aria-label="Tasks" aria-live="polite">
          {BANDS.map(([band, label]) => {
            const banded = rows(band);
            if (banded.length === 0) return null;
            const title = (
              <>
                <span className="task-band-label">{label}</span>
                <span className="task-band-count">{banded.length}</span>
              </>
            );
            // Idle is the tail you scroll past, so it collapses — natively, no JS.
            // ponytail: ships open; a closed <details> drops its rows out of the
            // accessibility tree. Flip the default only with the row queries in
            // Dashboard.test.tsx.
            return band === "idle" ? (
              <details key={band} className="task-band idle-band" data-tier={band} open>
                <summary className="task-band-title">{title}</summary>
                <div className="task-list">{banded}</div>
              </details>
            ) : (
              <section key={band} className="task-band" data-tier={band}>
                <div className="task-band-title">{title}</div>
                <div className="task-list">{banded}</div>
              </section>
            );
          })}
        </section>
      ) : (
        <p className="empty">
          {selectedProject
            ? `No tasks in ${selectedProject} yet — start one below.`
            : "All quiet — start a new task below."}
        </p>
      )}
    </>
  );
}
