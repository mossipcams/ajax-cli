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
import { filterByProject, relativeTime, sortCards, taskRowStatus } from "@/shared/lib/state";
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
  const statusInfo = taskRowStatus(card);
  const tone = statusInfo.tone;
  const wrapRef = useRef<HTMLDivElement>(null);
  const visibleActions = visibleTaskActions(card.actions);
  const time = card.last_activity_unix_secs;
  const inlineActions = visibleActions.filter(
    (action) =>
      !action.destructive && action.action !== "create-pr" && action.action !== "merge-pr",
  );
  const revealAction = visibleActions.find((action) => action.action === "drop");
  const [dragging, setDragging] = useState(false);

  useSwipeReveal(wrapRef, revealAction
    ? {
        getInitialOffset: () => offset,
        onOffset: (next) => onOffset(card.qualified_handle, next),
        onOpenChange: (open) => onRevealSettled(card.qualified_handle, open),
        onDragChange: setDragging,
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

  const isInlineActionTarget = (event: { target: EventTarget | null }) =>
    event.target instanceof Element && Boolean(event.target.closest(".task-row-actions"));

  const className = ["task-row", `tone-${tone}`, offset > 0 ? "is-revealed" : "", "ph-no-autocapture"]
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
      className={[
        "task-row-wrap",
        `tone-${tone}`,
        revealAction ? "has-reveal" : "",
        offset > 0 ? "is-revealed-wrap" : "",
      ]
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
      <div
        role="button"
        tabIndex={0}
        className={className}
        data-ph-no-autocapture=""
        data-handle={card.qualified_handle}
        style={{
          transform: `translateX(-${offset}px)`,
          transition: dragging ? "background 120ms var(--ease)" : undefined,
        }}
        onClick={(event) => {
          if (isInlineActionTarget(event)) return;
          handleTap();
        }}
        onKeyDown={(event) => {
          if (event.key !== "Enter" && event.key !== " ") return;
          if (isInlineActionTarget(event)) return;
          event.preventDefault();
          handleTap();
        }}
      >
        <div className="task-row-line">
          <span className="task-row-title">{card.title || card.qualified_handle}</span>
          {time ? (
            <span className="task-row-time">{relativeTime(time, nowSecs)}</span>
          ) : null}
        </div>
        <div className="task-row-line task-row-line-meta">
          <span className={`status-dot task-row-marker tone-${tone}`} aria-hidden="true" />
          <span className="task-row-meta">
            {statusInfo.label ? (
              <span className="task-row-status">{statusInfo.label}</span>
            ) : null}
            {card.title ? (
              <span className="task-row-handle">{card.qualified_handle}</span>
            ) : null}
          </span>
          {inlineActions.length > 0 ? (
            <div className="task-row-actions">
              <ActionBar
                actions={inlineActions}
                handle={card.qualified_handle}
                onCockpit={onCockpit}
                onResult={onResult}
                onMutated={onMutated}
                pendingConfirmAction={pendingConfirmAction}
                onCancelPendingConfirm={onCancelPendingConfirm}
              />
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
});

export default function TaskList({
  cockpit,
  selectedProject = null,
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

  const { needsYou, otherTasks } = useMemo(() => {
    const needs: BrowserTaskCard[] = [];
    const others: BrowserTaskCard[] = [];
    for (const card of calm) {
      if (card.attention === "needs-you" || card.attention === "review") {
        needs.push(card);
      } else {
        others.push(card);
      }
    }
    return { needsYou: needs, otherTasks: others };
  }, [calm]);

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

  const renderRow = (card: BrowserTaskCard) => (
    <TaskRow
      key={card.qualified_handle}
      card={card}
      offset={offsets[card.qualified_handle] ?? 0}
      {...rowProps}
    />
  );

  return (
    <>
      {calm.length > 0 ? (
        <section className="tasks" aria-label="Tasks" aria-live="polite">
          <div className="task-list">
            {needsYou.length > 0 ? <h2 className="task-group-heading">Needs you</h2> : null}
            {needsYou.map(renderRow)}
            {needsYou.length > 0 && otherTasks.length > 0 ? (
              <h2 className="task-group-heading">Other tasks</h2>
            ) : null}
            {otherTasks.map(renderRow)}
          </div>
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
