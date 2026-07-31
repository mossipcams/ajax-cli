import { useEffect, useMemo, useState } from "react";
import type {
  AttentionBand,
  BrowserCockpitView,
  BrowserTaskCard,
  ConnectionState,
} from "@/shared/lib/types";
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
import RepoPanel from "@/features/repositories/RepoPanel";
import SystemPanel from "./SystemPanel";

// THESIS: Dashboard is a button lattice — every safe intent is one tap, not a
// ledger with actions tucked under titles. OWN-WORLD: Soft Charcoal steps, Soft
// Steel Blue primary pills, amber remediation; Ajax Cockpit tokens unchanged.
// STORY: Scan bands, tap Fix CI / Review / Ship without opening the terminal.
// FIRST VIEWPORT: Band → one identity scan line → full-width primary pill →
// secondary pill row. The primary is the cell's largest object.
// FORM: Control-panel lattice (seed a3c11e37) + composition B primary-key.
// FINISH: Drop stays on detail; Resume/Open filtered; browser owns no task truth.

interface Props {
  cockpit: BrowserCockpitView;
  connection: ConnectionState;
  selectedProject?: string | null;
  onSelectProject?: (project: string | null) => void;
  onOpenTask?: (handle: string) => void;
  onOpenSettings?: () => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
}

type RowDispatch = Pick<Props, "onCockpit" | "onResult" | "onMutated" | "onOpenTask">;

interface TaskRowProps extends RowDispatch {
  card: BrowserTaskCard;
  nowSecs: number;
}

// Requested operator hierarchy. NOTE this puts "running now" ahead of "ready for
// action", inverting the order shipped in 6b3c0e5; band *membership* is still
// Rust's `card.attention` either way.
const BANDS: Array<[AttentionBand, string]> = [
  ["needs-you", "Needs attention"],
  ["active", "Running now"],
  ["review", "Ready for action"],
  ["idle", "Recent"],
];

function TaskRow({ card, nowSecs, onOpenTask, onCockpit, onResult, onMutated }: TaskRowProps) {
  const meta = statusMeta(card.status);
  const quiet = isQuiet(card, nowSecs);
  // Destructive stays on task detail: Drop is never the next step you want a
  // thumb to find while scanning a list.
  const actions = visibleTaskActions(card.actions).filter((action) => !action.destructive);
  // Say what the task is doing, always. The server's explanation is richer than
  // the bare status word, so it wins; the status word is the fallback rather
  // than a second chip repeating it.
  const explanation =
    card.status_explanation && card.status_explanation.toLowerCase() !== meta.label.toLowerCase()
      ? card.status_explanation
      : meta.label;

  const title = card.title || card.qualified_handle;
  const statusLine = quiet
    ? `Stale ${formatDuration(nowSecs - card.last_activity_unix_secs)} — no output`
    : explanation;

  return (
    <div
      className={`task-row tone-${meta.tone}${quiet ? " is-quiet" : ""}`}
      data-handle={card.qualified_handle}
      data-testid={`task-row-${card.qualified_handle}`}
    >
      <button
        type="button"
        className="task-row-tap"
        aria-label={`${title}. ${card.qualified_handle}. ${statusLine}`}
        onClick={() => onOpenTask?.(card.qualified_handle)}
      >
        <span className={`status-dot tone-${meta.tone}`} aria-hidden="true" />
        <span className="task-row-scan">
          <span className="task-row-handle">{card.qualified_handle}</span>
          <span className="task-row-title">{title}</span>
          <span className={quiet ? "task-row-quiet" : "task-row-note"}>{statusLine}</span>
          {card.last_activity_unix_secs ? (
            <span className="task-row-time">
              {relativeTime(card.last_activity_unix_secs, nowSecs)}
            </span>
          ) : null}
        </span>
      </button>

      {actions.length > 0 ? (
        <div className="task-row-actions" data-testid="task-row-actions">
          <ActionBar
            actions={actions}
            handle={card.qualified_handle}
            layout="primary-key"
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
  connection,
  selectedProject = null,
  onSelectProject,
  onOpenTask,
  onOpenSettings,
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

  const repos = cockpit.repos?.repos ?? [];
  // On a repo route the section collapses to that one repo — a list of one is
  // noise, but its counts are the page's subject.
  const scopedRepos = selectedProject
    ? repos.filter((repo) => repo.name === selectedProject)
    : repos;

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
              <details key={band} className="task-band idle-band" open>
                <summary className="task-band-title">{title}</summary>
                <div className="task-list">{banded}</div>
              </details>
            ) : (
              <section key={band} className="task-band">
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

      <RepoPanel
        repos={scopedRepos}
        selectedProject={selectedProject}
        onSelectProject={onSelectProject}
      />

      <SystemPanel
        backend={cockpit.backend}
        connection={connection}
        taskCount={cockpit.cards.length}
        onOpenSettings={onOpenSettings}
      />
    </>
  );
}
