import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { BrowserCockpitView, BrowserTaskCard } from "@/shared/lib/types";
import { filterByProject, relativeTime, sortCards, statusMeta } from "@/shared/lib/state";
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
  const rowRef = useRef<HTMLButtonElement>(null);
  // The primary action rides behind the row as a swipe reveal; tapping the row
  // opens the task detail where every action lives. One gesture, one surface.
  const revealAction = visibleTaskActions(card.actions)[0];

  useSwipeReveal(rowRef, revealAction
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

  const className = ["task-row", `tone-${meta.tone}`, offset > 0 ? "is-revealed" : ""]
    .filter(Boolean)
    .join(" ");

  return (
    <div className="task-row-wrap" data-handle={card.qualified_handle}>
      {revealAction ? (
        <div className="task-row-reveal" style={{ width: SWIPE_REVEAL_WIDTH }}>
          <ActionBar
            actions={[revealAction]}
            handle={card.qualified_handle}
            onCockpit={onCockpit}
            onResult={onResult}
            onMutated={onMutated}
          />
        </div>
      ) : null}
      <button
        ref={rowRef}
        type="button"
        className={className}
        data-handle={card.qualified_handle}
        style={{ transform: `translateX(-${offset}px)` }}
        onClick={handleTap}
      >
        <span className={`status-dot tone-${meta.tone}`} aria-hidden="true" />
        <div className="task-row-main">
          <span className="task-row-title">{card.title || card.qualified_handle}</span>
          {card.title ? <span className="task-row-handle">{card.qualified_handle}</span> : null}
          {card.status_explanation &&
          card.status_explanation.toLowerCase() !== meta.label.toLowerCase() ? (
            <span className="task-row-sub">{card.status_explanation}</span>
          ) : null}
        </div>
        <span className="task-row-side">
          <span className="task-row-status">{meta.label}</span>
          {card.last_activity_unix_secs ? (
            <span className="task-row-time">
              {relativeTime(card.last_activity_unix_secs, nowSecs)}
            </span>
          ) : null}
        </span>
        <span className="task-row-chevron">›</span>
      </button>
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

  useEffect(() => {
    const timer = setInterval(() => setNowSecs(Math.floor(Date.now() / 1000)), 60_000);
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

  const attentionByRepo = useMemo(
    () =>
      new Map(
        (cockpit.repos?.repos ?? []).map((repo) => [repo.name, repo.attention_items ?? 0]),
      ),
    [cockpit.repos?.repos],
  );

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

  const active = useMemo(() => calm.filter((card) => card.status !== "idle"), [calm]);
  const idle = useMemo(() => calm.filter((card) => card.status === "idle"), [calm]);

  const rowProps = {
    nowSecs,
    onOffset: setOffset,
    onOpenTask,
    onCockpit,
    onResult,
    onMutated,
  };

  const band = (cards: BrowserTaskCard[]) => (
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
            const count = attentionByRepo.get(project) ?? 0;
            return (
              <button
                key={project}
                type="button"
                className={`project-pill${selectedProject === project ? " is-active" : ""}`}
                aria-label={count ? `${project} — ${count} need attention` : project}
                aria-current={selectedProject === project ? "true" : undefined}
                onClick={() => onSelectProject?.(project)}
              >
                {project}
                {count ? (
                  <span className="pill-badge" aria-hidden="true">
                    {count}
                  </span>
                ) : null}
              </button>
            );
          })}
        </nav>
      ) : null}

      {calm.length > 0 ? (
        <section className="tasks" aria-label="Tasks" aria-live="polite">
          {active.length > 0 ? (
            <section className="task-band">
              <div className="task-band-title">
                <span className="task-band-label">Active</span>
                <span className="task-band-count">{active.length}</span>
              </div>
              {band(active)}
            </section>
          ) : null}
          {idle.length > 0 ? (
            // ponytail: ships open — a closed <details> drops its rows out of the
            // accessibility tree. Flip to collapsed-by-default only together with
            // the row queries in TaskList.test.tsx.
            <details className="task-band idle-band" open>
              <summary className="task-band-title">
                <span className="task-band-label">Idle</span>
                <span className="task-band-count">{idle.length}</span>
              </summary>
              {band(idle)}
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
