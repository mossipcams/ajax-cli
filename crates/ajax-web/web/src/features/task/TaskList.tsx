import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type CSSProperties,
} from "react";
import type { BrowserCockpitView, BrowserTaskCard } from "@/shared/lib/types";
import { filterByProject, relativeTime, sortCards, statusMeta } from "@/shared/lib/state";
import { visibleTaskActions } from "./taskActions";
import ActionBar from "./ActionBar";
import type { PendingConfirmRequest } from "./ActionBar";
import { useSwipeReveal } from "@/shared/hooks/useSwipeReveal";
import {
  REVEAL_AUTO_HIDE_MS,
  SWIPE_REVEAL_WIDTH,
  SWIPE_REVEAL_WIDTH_VAR,
} from "@/shared/gestures/swipeReveal";

interface Props {
  cockpit: BrowserCockpitView;
  selectedProject?: string | null;
  onSelectProject?: (project: string | null) => void;
  onOpenTask?: (handle: string) => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: {
      onUndo?: () => void;
      onCommit?: () => void;
      pendingConfirm?: PendingConfirmRequest;
    },
  ) => void;
  onMutated?: () => void;
  pendingConfirmAction?: string | null;
  onCancelPendingConfirm?: () => void;
}

interface ActionProps {
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: {
      onUndo?: () => void;
      onCommit?: () => void;
      pendingConfirm?: PendingConfirmRequest;
    },
  ) => void;
  onMutated?: () => void;
  pendingConfirmAction?: string | null;
  onCancelPendingConfirm?: () => void;
}

interface TaskRowProps extends ActionProps {
  card: BrowserTaskCard;
  nowSecs: number;
  offset: number;
  onOffset: (handle: string, offset: number) => void;
  onRevealSettled: (handle: string, open: boolean) => void;
  onOpenTask?: (handle: string) => void;
}

const TaskRow = memo(function TaskRow({
  card,
  nowSecs,
  offset,
  onOffset,
  onRevealSettled,
  onOpenTask,
  onCockpit,
  onResult,
  onMutated,
  pendingConfirmAction = null,
  onCancelPendingConfirm,
}: TaskRowProps) {
  const meta = statusMeta(card.status);
  const wrapRef = useRef<HTMLDivElement>(null);
  // The primary action rides behind the row as a swipe reveal; tapping the row
  // opens the task detail where every action lives. One gesture, one surface.
  const revealAction = visibleTaskActions(card.actions)[0];

  useSwipeReveal(wrapRef, revealAction
    ? {
        getInitialOffset: () => offset,
        onOffset: (next) => onOffset(card.qualified_handle, next),
        onOpenChange: (open) => onRevealSettled(card.qualified_handle, open),
        ignoreSelector: ".task-row-reveal",
      }
    : {});

  const handleTap = () => {
    if (offset > 0) {
      onOffset(card.qualified_handle, 0);
      return;
    }
    onOpenTask?.(card.qualified_handle);
  };

  const className = ["task-row", `tone-${meta.tone}`, offset > 0 ? "is-revealed" : "", "ph-no-autocapture"]
    .filter(Boolean)
    .join(" ");

  const closeRevealUnlessAction = (event: MouseEvent<HTMLDivElement>) => {
    if (offset <= 0) return;
    if ((event.target as Element).closest(".task-row-reveal")) return;
    onOffset(card.qualified_handle, 0);
  };

  const closeRevealOnKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    closeRevealUnlessAction(event as unknown as MouseEvent<HTMLDivElement>);
  };

  const wrapRevealDismiss =
    offset > 0
      ? {
          role: "button" as const,
          tabIndex: 0,
          "aria-label": "Close revealed actions",
          onClick: closeRevealUnlessAction,
          onKeyDown: closeRevealOnKeyDown,
        }
      : {};

  const wrapStyle = revealAction
    ? ({ [SWIPE_REVEAL_WIDTH_VAR]: `${SWIPE_REVEAL_WIDTH}px` } as CSSProperties)
    : undefined;

  return (
    <div
      ref={wrapRef}
      className={["task-row-wrap", revealAction ? "has-reveal" : "", offset > 0 ? "is-revealed-wrap" : ""]
        .filter(Boolean)
        .join(" ")}
      data-handle={card.qualified_handle}
      data-testid={`task-row-wrap-${card.qualified_handle}`}
      style={wrapStyle}
      {...wrapRevealDismiss}
    >
      {revealAction ? (
        <div className="task-row-reveal" aria-hidden={offset <= 0}>
          <ActionBar
            actions={[revealAction]}
            handle={card.qualified_handle}
            onCockpit={onCockpit}
            onResult={onResult}
            onMutated={onMutated}
            pendingConfirmAction={pendingConfirmAction}
            onCancelPendingConfirm={onCancelPendingConfirm}
          />
        </div>
      ) : null}
      <button
        type="button"
        className={className}
        data-ph-no-autocapture=""
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
});

export default function TaskList({
  cockpit,
  selectedProject = null,
  onSelectProject,
  onOpenTask,
  onCockpit,
  onResult,
  onMutated,
  pendingConfirmAction = null,
  onCancelPendingConfirm,
}: Props) {
  const [offsets, setOffsets] = useState<Record<string, number>>({});
  const [pendingConfirmHandle, setPendingConfirmHandle] = useState<string | null>(null);
  const [nowSecs, setNowSecs] = useState(() => Math.floor(Date.now() / 1000));
  const [stableOrder, setStableOrder] = useState<string[]>([]);
  const autoHideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const revealedHandleRef = useRef<string | null>(null);

  const clearAutoHide = useCallback(() => {
    if (autoHideTimerRef.current !== null) {
      clearTimeout(autoHideTimerRef.current);
      autoHideTimerRef.current = null;
    }
  }, []);

  useEffect(() => () => clearAutoHide(), [clearAutoHide]);

  useEffect(() => {
    if (pendingConfirmAction === null) setPendingConfirmHandle(null);
  }, [pendingConfirmAction]);

  useEffect(() => {
    if (pendingConfirmHandle === null) return;
    if ((offsets[pendingConfirmHandle] ?? 0) < SWIPE_REVEAL_WIDTH) return;
    clearAutoHide();
  }, [pendingConfirmHandle, offsets, clearAutoHide]);

  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | undefined;

    const syncNowSecs = () => setNowSecs(Math.floor(Date.now() / 1000));

    const startTicker = () => {
      if (timer !== undefined) return;
      syncNowSecs();
      timer = setInterval(syncNowSecs, 60_000);
    };

    const stopTicker = () => {
      if (timer === undefined) return;
      clearInterval(timer);
      timer = undefined;
    };

    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") startTicker();
      else stopTicker();
    };

    if (document.visibilityState === "visible") startTicker();
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      document.removeEventListener("visibilitychange", onVisibilityChange);
      stopTicker();
    };
  }, []);

  const scheduleAutoHide = useCallback(
    (handle: string) => {
      clearAutoHide();
      revealedHandleRef.current = handle;
      if (pendingConfirmHandle === handle) return;
      autoHideTimerRef.current = setTimeout(() => {
        autoHideTimerRef.current = null;
        revealedHandleRef.current = null;
        setOffsets((prev) => {
          if ((prev[handle] ?? 0) <= 0) return prev;
          return { ...prev, [handle]: 0 };
        });
      }, REVEAL_AUTO_HIDE_MS);
    },
    [clearAutoHide, pendingConfirmHandle],
  );

  useEffect(() => {
    const handle = revealedHandleRef.current;
    if (!handle || pendingConfirmHandle !== null) return;
    if ((offsets[handle] ?? 0) < SWIPE_REVEAL_WIDTH) return;
    scheduleAutoHide(handle);
  }, [pendingConfirmHandle, offsets, scheduleAutoHide]);

  const setOffset = useCallback(
    (handle: string, offset: number) => {
      if (offset <= 0) {
        if (revealedHandleRef.current === handle) {
          clearAutoHide();
          revealedHandleRef.current = null;
        }
        setOffsets((prev) => ({ ...prev, [handle]: 0 }));
        return;
      }
      if (revealedHandleRef.current !== handle) clearAutoHide();
      setOffsets((prev) => {
        const next: Record<string, number> = {};
        for (const key of Object.keys(prev)) next[key] = 0;
        next[handle] = offset;
        return next;
      });
    },
    [clearAutoHide],
  );

  const onRevealSettled = useCallback(
    (handle: string, open: boolean) => {
      if (open) scheduleAutoHide(handle);
      else if (revealedHandleRef.current === handle) {
        clearAutoHide();
        revealedHandleRef.current = null;
      }
    },
    [clearAutoHide, scheduleAutoHide],
  );

  const handleResult = useCallback(
    (
      message: string,
      output: string | null | undefined,
      isError: boolean,
      options?: {
        onUndo?: () => void;
        onCommit?: () => void;
        pendingConfirm?: PendingConfirmRequest;
      },
    ) => {
      if (options?.pendingConfirm) {
        setPendingConfirmHandle(options.pendingConfirm.handle);
      } else {
        setPendingConfirmHandle(null);
      }
      onResult?.(message, output, isError, options);
    },
    [onResult],
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
    onRevealSettled,
    onOpenTask,
    onCockpit,
    onResult: handleResult,
    onMutated,
    pendingConfirmAction,
    onCancelPendingConfirm,
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
