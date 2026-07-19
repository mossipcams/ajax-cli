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

interface TaskRowProps {
  card: BrowserTaskCard;
  isInbox: boolean;
  nowSecs: number;
  offset: number;
  onOffset: (handle: string, offset: number) => void;
  onOpenTask?: (handle: string) => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
}

function TaskRow({
  card,
  isInbox,
  nowSecs,
  offset,
  onOffset,
  onOpenTask,
  onCockpit,
  onResult,
  onMutated,
}: TaskRowProps) {
  const meta = statusMeta(card.status);
  const revealAction = visibleTaskActions(card.actions)[0];
  const rowRef = useRef<HTMLButtonElement>(null);

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
        className={`task-row tone-${meta.tone}${isInbox ? " is-inbox" : ""}${offset > 0 ? " is-revealed" : ""}`}
        data-handle={card.qualified_handle}
        style={{ transform: `translateX(-${offset}px)` }}
        onClick={handleTap}
      >
        <span className={`status-dot tone-${meta.tone}`} aria-hidden="true" />
        <div className="task-row-main">
          <span className="task-row-handle">{card.qualified_handle}</span>
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

  const cardsByHandle = useMemo(
    () => new Map(cockpit.cards.map((card) => [card.qualified_handle, card])),
    [cockpit.cards],
  );

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

  const inboxItems = useMemo(
    () =>
      (cockpit.inbox?.items ?? [])
        .slice()
        .sort((a, b) => (a.severity ?? 999) - (b.severity ?? 999))
        .map((item) => ({ item, card: cardsByHandle.get(item.task_handle) }))
        .filter(
          (entry): entry is { item: typeof entry.item; card: NonNullable<typeof entry.card> } =>
            entry.card != null && (!selectedProject || entry.card.repo === selectedProject),
        ),
    [cockpit.inbox?.items, cardsByHandle, selectedProject],
  );

  const inboxHandles = useMemo(
    () => new Set(inboxItems.map((entry) => entry.card.qualified_handle)),
    [inboxItems],
  );

  const groups = useMemo(() => {
    const visible = filterByProject(cockpit.cards, selectedProject).filter(
      (card) => !inboxHandles.has(card.qualified_handle),
    );
    const byRepo = new Map<string, typeof visible>();
    for (const card of visible) {
      if (!byRepo.has(card.repo)) byRepo.set(card.repo, []);
      byRepo.get(card.repo)!.push(card);
    }
    return [...byRepo.keys()]
      .sort()
      .map((repo) => ({ repo, cards: sortCards(byRepo.get(repo)!, stableOrder) }));
  }, [cockpit.cards, selectedProject, inboxHandles, stableOrder]);

  useEffect(() => {
    const next = groups.flatMap((group) => group.cards.map((card) => card.qualified_handle));
    setStableOrder((prev) => {
      if (next.length === prev.length && next.every((handle, i) => handle === prev[i])) {
        return prev;
      }
      return next;
    });
  }, [groups]);

  const calmCount = useMemo(
    () => groups.reduce((sum, group) => sum + group.cards.length, 0),
    [groups],
  );
  const showRepoTitles = !selectedProject && groups.length > 1;
  const visibleCount = filterByProject(cockpit.cards, selectedProject).length;

  const rowProps = {
    nowSecs,
    onOffset: setOffset,
    onOpenTask,
    onCockpit,
    onResult,
    onMutated,
  };

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

      {inboxItems.length > 0 ? (
        <section className="group inbox" aria-label="Needs you" aria-live="polite">
          <div className="section-head attention">
            <span className="section-head-title">Needs you</span>
            <span className="section-head-count">{inboxItems.length}</span>
          </div>
          <div className="task-list">
            {inboxItems.map((entry) => (
              <TaskRow
                key={entry.card.qualified_handle}
                card={entry.card}
                isInbox
                offset={offsets[entry.card.qualified_handle] ?? 0}
                {...rowProps}
              />
            ))}
          </div>
        </section>
      ) : null}

      {calmCount > 0 ? (
        <section className="group tasks" aria-label="Tasks" aria-live="polite">
          <div className="section-head">
            <span className="section-head-title">{selectedProject ?? "Tasks"}</span>
            <span className="section-head-count">{calmCount}</span>
          </div>
          {groups.map((group) => (
            <section key={group.repo} className="task-group">
              {showRepoTitles ? <div className="task-group-title">{group.repo}</div> : null}
              <div className="task-list">
                {group.cards.map((card) => (
                  <TaskRow
                    key={card.qualified_handle}
                    card={card}
                    isInbox={false}
                    offset={offsets[card.qualified_handle] ?? 0}
                    {...rowProps}
                  />
                ))}
              </div>
            </section>
          ))}
        </section>
      ) : null}

      {visibleCount === 0 ? (
        <p className="empty">
          {selectedProject
            ? `No tasks in ${selectedProject} yet — start one below.`
            : "All quiet — start a new task below."}
        </p>
      ) : null}
    </>
  );
}
