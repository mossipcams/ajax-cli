import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { BrowserCockpitView, BrowserTaskCard, AttentionBand } from "@/shared/lib/types";
import {
  filterByProject,
  formatDuration,
  isQuiet,
  relativeTime,
  reposWithFault,
  sortCards,
  statusMeta,
} from "@/shared/lib/state";
import { visibleTaskActions } from "./taskActions";
import ActionBar from "./ActionBar";
import { useSwipeReveal } from "@/shared/hooks/useSwipeReveal";
import { SWIPE_REVEAL_WIDTH } from "@/shared/gestures/swipeReveal";

interface Props {
  cockpit: BrowserCockpitView;
  selectedProject?: string | null;
  onSelectProject?: (project: string | null) => void;
  onOpenTask?: (handle: string) => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
}

interface ActionProps {
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
}

interface TaskRowProps extends ActionProps {
  card: BrowserTaskCard;
  nowSecs: number;
  offset: number;
  onOffset: (handle: string, offset: number) => void;
  onOpenTask?: (handle: string) => void;
}

function TaskRow({
  card,
  nowSecs,
  offset,
  onOffset,
  onOpenTask,
  onCockpit,
  onResult,
  onMutated,
}: TaskRowProps) {
  const meta = statusMeta(card.status);
  const quiet = isQuiet(card, nowSecs);
  const rowRef = useRef<HTMLDivElement>(null);
  const controls = visibleTaskActions(card.actions).filter((a) => !a.destructive);
  const inlineAction = controls[0] ?? null;
  const revealActions = controls.slice(1);

  useSwipeReveal(rowRef, revealActions.length > 0
    ? {
        onOffset: (next) => onOffset(card.qualified_handle, next),
        onOpenChange: () => {},
      }
    : {});

  const handleTap = () => {
    if (offset > 0) {
      onOffset(card.qualified_handle, 0);
      return;
    }
    onOpenTask?.(card.qualified_handle);
  };

  const className = [
    "task-row",
    `tone-${meta.tone}`,
    offset > 0 ? "is-revealed" : "",
    quiet ? "is-quiet" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className="task-row-wrap" data-handle={card.qualified_handle}>
      {revealActions.length > 0 ? (
        <div className="task-row-reveal" style={{ width: SWIPE_REVEAL_WIDTH }}>
          <ActionBar
            actions={revealActions}
            handle={card.qualified_handle}
            onCockpit={onCockpit}
            onResult={onResult}
            onMutated={onMutated}
          />
        </div>
      ) : null}
      <div
        ref={rowRef}
        className={className}
        data-handle={card.qualified_handle}
        style={{ transform: `translateX(-${offset}px)` }}
      >
        <button type="button" className="task-row-tap" data-handle={card.qualified_handle} onClick={handleTap}>
          <span className={`status-dot tone-${meta.tone}`} aria-hidden="true" />
          <div className="task-row-main">
            <span className="task-row-title">{card.title || card.qualified_handle}</span>
            {card.title ? <span className="task-row-handle">{card.qualified_handle}</span> : null}
            {quiet ? (
              <span className="task-row-quiet">
                Quiet {formatDuration(nowSecs - card.last_activity_unix_secs)} — no output
              </span>
            ) : card.status_explanation &&
              card.status_explanation.toLowerCase() !== meta.label.toLowerCase() ? (
              <span className="task-row-sub">{card.status_explanation}</span>
            ) : null}
          </div>
          <span className="task-row-side">
            {card.last_activity_unix_secs ? (
              <span className="task-row-time">
                {relativeTime(card.last_activity_unix_secs, nowSecs)}
              </span>
            ) : null}
          </span>
          <span className="task-row-chevron">›</span>
        </button>
        <div className="task-row-control">
          {inlineAction ? (
            <ActionBar
              actions={[inlineAction]}
              handle={card.qualified_handle}
              onCockpit={onCockpit}
              onResult={onResult}
              onMutated={onMutated}
            />
          ) : (
            <button
              type="button"
              className="row-control"
              onClick={() => onOpenTask?.(card.qualified_handle)}
            >
              {card.attention === "needs-you" ? "Answer" : "Open"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

export default function TaskList({
  cockpit,
  selectedProject = null,
  onSelectProject,
  onOpenTask,
  onCockpit,
  onResult,
  onMutated,
}: Props) {
  const [offsets, setOffsets] = useState<Record<string, number>>({});
  const [nowSecs, setNowSecs] = useState(() => Math.floor(Date.now() / 1000));
  const [stableOrder, setStableOrder] = useState<string[]>([]);

  // Quiet detection turns on a 4-minute boundary, so the clock must tick faster
  // than the 60s row-time refresh to flip a running row to "quiet" on time.
  useEffect(() => {
    const timer = setInterval(() => setNowSecs(Math.floor(Date.now() / 1000)), 30_000);
    return () => clearInterval(timer);
  }, []);

  const setOffset = useCallback((handle: string, offset: number) => {
    setOffsets((prev) => ({ ...prev, [handle]: offset }));
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
  // carries health, not a metric. Faults are counted across the whole fleet, not
  // the project-filtered slice, so the dot is honest whatever pill is active.
  const faultRepos = useMemo(() => reposWithFault(cockpit.cards), [cockpit.cards]);

  // Rust ranks the cards; the browser only keeps that order stable across polls
  // so rows don't reshuffle under the operator's thumb.
  const calm = useMemo(
    () => sortCards(filterByProject(cockpit.cards, selectedProject), stableOrder),
    [cockpit.cards, selectedProject, stableOrder],
  );

  useEffect(() => {
    const next = calm.map((card) => card.qualified_handle);
    setStableOrder((prev) => {
      if (next.length === prev.length && next.every((handle, i) => handle === prev[i])) {
        return prev;
      }
      return next;
    });
  }, [calm]);

  // Group by the server-owned attention band; ordering within each band stays Rust's.
  const needsYou = useMemo(() => calm.filter((card) => card.attention === "needs-you"), [calm]);
  const review = useMemo(() => calm.filter((card) => card.attention === "review"), [calm]);
  const active = useMemo(() => calm.filter((card) => card.attention === "active"), [calm]);
  const idle = useMemo(() => calm.filter((card) => card.attention === "idle"), [calm]);
  const rowProps = {
    nowSecs,
    onOffset: setOffset,
    onOpenTask,
    onCockpit,
    onResult,
    onMutated,
  };

  const cardList = (cards: BrowserTaskCard[]) => (
    <div className="task-list">
      {cards.map((card) => (
        <TaskRow
          key={card.qualified_handle}
          card={card}
          offset={offsets[card.qualified_handle] ?? 0}
          {...rowProps}
        />
      ))}
    </div>
  );

  const tier = (attentionBand: AttentionBand, label: string, cards: BrowserTaskCard[]) =>
    cards.length > 0 ? (
      <section className="task-band" data-tier={attentionBand}>
        <div className="task-band-title">
          <span className="task-band-label">{label}</span>
          <span className="task-band-count">{cards.length}</span>
        </div>
        {cardList(cards)}
      </section>
    ) : null;

  return (
    <>
      {projects.length > 0 ? (
        <nav className="project-nav" aria-label="Projects">
          <span className="project-nav-label">Projects</span>
          <button
            type="button"
            className={`project-pill${!selectedProject ? " is-active" : ""}`}
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

      {calm.length > 0 ? (
        <section className="tasks" aria-label="Tasks" aria-live="polite">
          {tier("needs-you", "Needs you", needsYou)}
          {tier("review", "Ready to review", review)}
          {tier("active", "Active", active)}
          {idle.length > 0 ? (
            // ponytail: ships open — a closed <details> drops its rows out of the
            // accessibility tree. Flip to collapsed-by-default only together with
            // the row queries in TaskList.test.tsx.
            <details className="task-band idle-band" data-tier="idle" open>
              <summary className="task-band-title">
                <span className="task-band-label">Idle</span>
                <span className="task-band-count">{idle.length}</span>
              </summary>
              {cardList(idle)}
            </details>
          ) : null}
        </section>
      ) : null}

      {calm.length === 0 ? (
        <p className="empty">
          {selectedProject
            ? `No tasks in ${selectedProject} yet — start one below.`
            : "All quiet — start a new task below."}
        </p>
      ) : null}
    </>
  );
}
