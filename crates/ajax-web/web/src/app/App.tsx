import { useEffect, useEffectEvent, useRef, useState } from "react";
import {
  dashboardHash,
  parseRoute,
  projectHash,
  sessionHash,
  settingsHash,
  taskDiffHash,
  taskHash,
} from "@/shared/lib/routes";
import {
  cockpitRefreshIntervalMs,
  REFRESH_INTERVAL_ACTIVE_MS,
  versionPollIntervalMs,
  type PollingRouteKind,
} from "@/shared/lib/polling";
import ConnectionStatus from "@/shared/ui/ConnectionStatus";
import ResultPanel from "@/shared/ui/ResultPanel";
import TaskList from "@/features/task/TaskList";
import TaskDetail from "@/features/task/TaskDetail";
import TaskLoadError from "@/features/task/TaskLoadError";
import DiffReview from "@/features/diff/DiffReview";
import SettingsView from "@/features/settings/SettingsView";
import SessionChat from "@/features/session/SessionChat";
import { useOrchestrationChatEnabled } from "@/features/session/sessionMode";
import {
  clearTaskTerminalPreferred,
  readTaskTerminalPreferred,
  writeTaskTerminalPreferred,
} from "@/features/session/taskViewPreference";
import NewTaskSheet from "@/features/task/NewTaskSheet";
import Skeleton from "@/shared/ui/Skeleton";
import AppViewport from "./AppViewport";
import AppShell from "./AppShell";
import RouteScroll from "./RouteScroll";
import { PULL_THRESHOLD } from "@/shared/gestures/pullToRefresh";
import { useHashRoute } from "@/shared/hooks/useHashRoute";
import { usePullToRefresh } from "@/shared/hooks/usePullToRefresh";
import { useVersionMonitor } from "@/shared/hooks/useVersionMonitor";
import { useCockpitResource } from "@/shared/hooks/useCockpitResource";
import { useTaskDetailResource } from "@/features/task/useTaskDetailResource";
import { useTaskOperationMutation } from "@/features/task/useTaskOperationMutation";
import {
  consumeSwipeEnterDirection,
  navigateHashWithEnter,
  swipeEnterClassName,
  type SwipeEnterDirection,
} from "@/shared/lib/swipeEnter";
import type { BrowserCockpitView, WebAction } from "@/shared/lib/types";
import {
  beginInteraction,
  capturePwaLaunch,
  capturePwaResume,
  captureRouteVisible,
  endTapToFeedback,
  endTapToOperationComplete,
  isNavigationPending,
  markNavigationStart,
} from "@/shared/lib/telemetry";
import {
  commitConfirmedAction,
  type DropUndoHandles,
} from "@/features/task/taskMutations";
import { checkHealth } from "@/shared/lib/api";
import { clearSessionOutbox } from "@/shared/lib/webSessionTransport";

/** Coalesce iOS focus/pageshow/visibility resume bursts into one recovery poll. */
const RESUME_DEBOUNCE_MS = 750;

type ResultState = {
  message: string;
  output?: string | null;
  isError: boolean;
  onUndo?: () => void;
  onCommit?: () => void;
};

type PendingConfirmState = {
  action: WebAction;
  handle: string;
  interactionId: string;
};

/** Task surfaces where Drop confirm/dismiss treat the operator as on this handle. */
function routeStillOnDroppedTask(
  current: ReturnType<typeof parseRoute>,
  handle: string,
): boolean {
  return (
    (current.kind === "task" || current.kind === "diff" || current.kind === "session") &&
    current.handle === handle
  );
}

export default function App() {
  const route = useHashRoute();
  const orchestrationChat = useOrchestrationChatEnabled();
  const {
    cockpit,
    connection,
    connectionDetail,
    loadCockpit,
    applyCockpit,
    applyConnectionError,
    markConnected,
  } = useCockpitResource();
  const selectedProject = route.kind === "project" ? (route.project ?? null) : null;
  const taskOpenHandle =
    route.kind === "task" || route.kind === "diff" || route.kind === "session"
      ? (route.handle ?? null)
      : null;
  const { detail, reload } = useTaskDetailResource(taskOpenHandle, {
    applyCockpit,
    applyConnectionError,
    markConnected,
  });
  const { updateAvailable, checkVersion } = useVersionMonitor();
  const executeOperation = useTaskOperationMutation();
  const [sheetOpen, setSheetOpen] = useState(false);
  const [result, setResult] = useState<ResultState | null>(null);
  const [pendingConfirm, setPendingConfirm] = useState<PendingConfirmState | null>(null);
  const dropTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dropResolvedRef = useRef(false);
  const dropHandles: DropUndoHandles = { dropTimerRef, dropResolvedRef };
  // Sticky leave latch: once the operator leaves the dropped task (including
  // during shell confirm, before Confirm), late Drop success must not go(#/).
  // Snapshotting location.hash only at API-completion races swipe/Back settle,
  // which delays the hash change by SWIPE_PAGE_COMMIT_MS.
  const dropLeaveLatchRef = useRef<{ handle: string; left: boolean } | null>(null);
  const reloadLatchRef = useRef(false);
  const [pullDistance, setPullDistance] = useState(0);
  const [documentVisibility, setDocumentVisibility] = useState<DocumentVisibilityState>(
    typeof document !== "undefined" ? document.visibilityState : "visible",
  );
  const [swipeEnter, setSwipeEnter] = useState<SwipeEnterDirection | null>(null);
  const outletSwipeRef = useRef<HTMLElement | null>(null);
  const cockpitRef = useRef(cockpit);
  cockpitRef.current = cockpit;
  const resumeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pwaLaunchCapturedRef = useRef(false);
  const hiddenAtRef = useRef<number | null>(null);
  const pendingPwaResumeRef = useRef<{
    hidden_ms: number;
    visibleAt: number;
    resumeToVisibleMs: number | null;
  } | null>(null);
  // Report what's live first, then the inventory size.
  const statusText = (() => {
    if (!cockpit.data) return "— loading";
    const running = cockpit.data.cards.filter((card) => card.status === "running").length;
    if (running) return `${running} running`;
    const total = cockpit.data.cards.length;
    return `${total} ${total === 1 ? "task" : "tasks"}`;
  })();

  function showResult(
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: {
      onUndo?: () => void;
      onCommit?: () => void;
      pendingConfirm?: PendingConfirmState;
    },
  ) {
    if (options?.pendingConfirm) {
      const pending = options.pendingConfirm;
      if (pending.action.action === "drop") {
        dropLeaveLatchRef.current = { handle: pending.handle, left: false };
      }
      setPendingConfirm(pending);
      return;
    }
    setResult({ message, output, isError, onUndo: options?.onUndo, onCommit: options?.onCommit });
  }

  function dismissPendingConfirm() {
    setPendingConfirm(null);
  }

  function cancelPendingConfirm() {
    if (!pendingConfirm) return;
    endTapToOperationComplete(pendingConfirm.interactionId, {
      ok: false,
      op: pendingConfirm.action.action,
      error_kind: "undo",
    });
    dropLeaveLatchRef.current = null;
    dismissPendingConfirm();
  }

  const cancelPendingConfirmOnRouteChange = useEffectEvent(() => {
    if (!pendingConfirm) return;
    if (routeStillOnDroppedTask(route, pendingConfirm.handle)) return;
    cancelPendingConfirm();
  });

  useEffect(() => {
    cancelPendingConfirmOnRouteChange();
  }, [route.kind, route.handle]);

  function expirePendingConfirm() {
    if (!pendingConfirm) return;
    endTapToOperationComplete(pendingConfirm.interactionId, {
      ok: false,
      error_kind: "confirm_timeout",
    });
    dropLeaveLatchRef.current = null;
    dismissPendingConfirm();
  }

  function commitPendingConfirm() {
    if (!pendingConfirm) return;
    const { action, handle, interactionId } = pendingConfirm;
    dismissPendingConfirm();
    // Drop's undo timer outlives ActionBar. Dismiss to dashboard only while the
    // operator is still on the dropped task — leave latch + live hash check.
    const stillOnDroppedTask = () => {
      if (dropLeaveLatchRef.current?.left) return false;
      return routeStillOnDroppedTask(parseRoute(window.location.hash), handle);
    };
    commitConfirmedAction(
      action,
      handle,
      interactionId,
      {
        onCockpit: applyCockpit,
        onResult: showResult,
        onMutated: () => {
          if (
            (route.kind === "task" || route.kind === "session") &&
            route.handle
          ) {
            reload();
          } else void loadCockpit();
        },
        isMounted: stillOnDroppedTask,
        onDismiss: () => {
          // Re-check at navigate time: API may have resolved before swipe settle
          // updated the hash, or after the leave latch flipped.
          if (!stillOnDroppedTask()) {
            dropLeaveLatchRef.current = null;
            return;
          }
          dropLeaveLatchRef.current = null;
          go(dashboardHash());
        },
      },
      dropHandles,
      executeOperation,
    );
  }

  function whenIdle(callback: () => void): number {
    if (typeof requestIdleCallback === "function") return requestIdleCallback(callback);
    return setTimeout(callback, 1) as unknown as number;
  }

  function cancelIdle(handle: number) {
    if (typeof cancelIdleCallback === "function") cancelIdleCallback(handle);
    else clearTimeout(handle);
  }

  function go(hash: string) {
    if (location.hash !== hash) {
      markNavigationStart(undefined, "hash");
    }
    location.hash = hash;
  }

  function openTask(handle: string, latestCockpit?: BrowserCockpitView) {
    const interactionId = beginInteraction("open_task");
    endTapToFeedback(interactionId, "nav_start");
    // Yield past this tap's INP next-paint before sync hash→TaskList teardown.
    // A single rAF still runs before paint and would keep INP ~400–500ms.
    // Only a provisioned (ACP) task can hold a session; an interactive task
    // keeps its agent in tmux, so chat would open on a socket the host refuses.
    window.setTimeout(() => {
      const sessionCapable = (latestCockpit ?? cockpitRef.current.data)?.cards?.some(
        (card) => card.qualified_handle === handle && card.session_capable,
      );
      const terminalPreferred = readTaskTerminalPreferred(handle);
      const hash =
        orchestrationChat && sessionCapable && !terminalPreferred
          ? sessionHash(handle)
          : taskHash(handle);
      markNavigationStart(undefined, "open_task");
      navigateHashWithEnter(hash, "left");
      endTapToOperationComplete(interactionId, { ok: true, op: "open_task" });
    }, 0);
  }

  const pullToRefreshRef = usePullToRefresh({
    onRefresh: () => loadCockpit(),
    onDistance: setPullDistance,
  });

  // The shell subscription below must mount exactly once, but its handlers need
  // the latest loadCockpit/checkVersion. Effect events are non-reactive, so they
  // give us that without making the subscription re-run.
  const onShellMount = useEffectEvent(() => {
    void loadCockpit();
    return whenIdle(() => void checkVersion());
  });
  const emitPendingPwaResume = useEffectEvent((cockpit_ok: boolean) => {
    const pending = pendingPwaResumeRef.current;
    if (!pending) {
      return;
    }
    const resume_to_cockpit_ms = Math.round(performance.now() - pending.visibleAt);
    capturePwaResume({
      hidden_ms: pending.hidden_ms,
      resume_to_visible_ms: pending.resumeToVisibleMs ?? resume_to_cockpit_ms,
      resume_to_cockpit_ms,
      resume_debounce_ms: RESUME_DEBOUNCE_MS,
      online: navigator.onLine,
      cockpit_ok,
    });
    pendingPwaResumeRef.current = null;
  });
  const onShellResume = useEffectEvent(async () => {
    void checkVersion();
    await loadCockpit({ trailing: true });
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    });
    const c = cockpitRef.current;
    const cockpit_ok =
      c.status === "ready" || (c.status === "stale" && c.data !== null);
    emitPendingPwaResume(cockpit_ok);
  });
  const scheduleShellResume = useEffectEvent(() => {
    if (resumeTimerRef.current !== null) clearTimeout(resumeTimerRef.current);
    resumeTimerRef.current = setTimeout(() => {
      resumeTimerRef.current = null;
      void onShellResume();
    }, RESUME_DEBOUNCE_MS);
  });
  const onShellVisibilityChange = useEffectEvent(() => {
    const wasHidden = documentVisibility === "hidden";
    const nowVisible = document.visibilityState === "visible";
    if (document.visibilityState === "hidden") {
      hiddenAtRef.current = performance.now();
    }
    setDocumentVisibility(document.visibilityState);
    if (nowVisible && wasHidden && hiddenAtRef.current !== null) {
      const visibleAt = performance.now();
      const hidden_ms = Math.round(visibleAt - hiddenAtRef.current);
      hiddenAtRef.current = null;
      pendingPwaResumeRef.current = {
        hidden_ms,
        visibleAt,
        resumeToVisibleMs: null,
      };
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          const pending = pendingPwaResumeRef.current;
          if (pending) {
            pending.resumeToVisibleMs = Math.round(
              performance.now() - pending.visibleAt,
            );
          }
        });
      });
    }
    if (nowVisible) {
      scheduleShellResume();
    }
  });
  // Shell listeners — mount once; immediate cockpit on mount, debounced recovery on resume.
  useEffect(() => {
    const idleHandle = onShellMount();
    const onResume = () => scheduleShellResume();
    const onVisibilityChange = () => onShellVisibilityChange();
    window.addEventListener("focus", onResume);
    window.addEventListener("pageshow", onResume);
    window.addEventListener("online", onResume);
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      cancelIdle(idleHandle);
      if (resumeTimerRef.current !== null) clearTimeout(resumeTimerRef.current);
      window.removeEventListener("focus", onResume);
      window.removeEventListener("pageshow", onResume);
      window.removeEventListener("online", onResume);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);

  const wasListRouteRef = useRef(route.kind === "dashboard" || route.kind === "project");
  useEffect(() => {
    const isListRoute = route.kind === "dashboard" || route.kind === "project";
    if (isListRoute && !wasListRouteRef.current) void loadCockpit();
    wasListRouteRef.current = isListRoute;
  }, [loadCockpit, route.kind]);

  // Adaptive cockpit / version intervals. Derive the scalar cadences first: an
  // inline object literal is a new value every render and could never be a
  // dependency, which is what forced the old suppression here.
  const fleetQuiet =
    cockpit.data !== null &&
    cockpit.data.cards.every((card) => (card.status || "").toLowerCase() === "idle");
  const pollingInput = {
    visibilityState: documentVisibility,
    routeKind: route.kind as PollingRouteKind,
    fleetQuiet,
  };
  const noCockpitProjection = cockpit.data === null;
  const hiddenStartupRetry = noCockpitProjection && cockpit.status === "error";
  const cockpitIntervalMs = hiddenStartupRetry
    ? REFRESH_INTERVAL_ACTIVE_MS
    : cockpitRefreshIntervalMs(pollingInput);
  const versionIntervalMs = versionPollIntervalMs(pollingInput);

  useEffect(() => {
    const cockpitTimer = window.setInterval(() => {
      if (!document.hidden || hiddenStartupRetry) void loadCockpit({ deferDuringGesture: true });
    }, cockpitIntervalMs);
    const versionTimer = window.setInterval(checkVersion, versionIntervalMs);
    return () => {
      window.clearInterval(cockpitTimer);
      window.clearInterval(versionTimer);
    };
  }, [checkVersion, cockpitIntervalMs, hiddenStartupRetry, loadCockpit, versionIntervalMs]);

  // Sheet is a list overlay only — clear on task/diff/settings (and any non-list
  // route), including a late reopen so swipe-back never remounts it.
  const sheetAllowed = route.kind === "dashboard" || route.kind === "project";
  useEffect(() => {
    if (sheetOpen && !sheetAllowed) {
      setSheetOpen(false);
    }
  }, [sheetAllowed, sheetOpen]);

  // Flip Drop leave latch as soon as React observes a non-dropped route so a
  // late Drop success cannot go(#/) after the operator has moved on.
  useEffect(() => {
    const latch = dropLeaveLatchRef.current;
    if (!latch) return;
    if (!routeStillOnDroppedTask(route, latch.handle)) latch.left = true;
  }, [route]);

  useEffect(() => {
    if (route.kind === "session" && !orchestrationChat) {
      go(route.handle ? taskHash(route.handle) : dashboardHash());
    }
  }, [route.kind, route.handle, orchestrationChat]);

  // A session on a task the host will not attach (interactive, or an agent with
  // no ACP entry point) would sit on a refused socket. Send it to the terminal.
  useEffect(() => {
    if (route.kind !== "session" || !route.handle) return;
    if (detail.status !== "ready" || !detail.data) return;
    if (detail.data.qualified_handle !== route.handle) return;
    if (detail.data.session_capable === false) go(taskHash(route.handle));
    else if (readTaskTerminalPreferred(route.handle)) go(taskHash(route.handle));
  }, [route.kind, route.handle, detail.status, detail.data]);

  useEffect(() => {
    const kind = route.kind;
    if (kind === "task" && route.handle) {
      document.title = `${route.handle} — Ajax`;
    } else if (kind === "session" && route.handle) {
      document.title = `${route.handle} — Ajax`;
    } else if (kind === "session") {
      document.title = "New session — Ajax";
    } else if (kind === "settings") {
      document.title = "Settings — Ajax";
    } else if (kind === "project" && route.project) {
      document.title = `${route.project} — Ajax`;
    } else {
      document.title = "Ajax";
    }
  }, [route]);

  useEffect(() => {
    const kind = route.kind;
    const contentReady =
      kind === "settings" ||
      (kind === "session" && (!route.handle || detail.status !== "loading")) ||
      (kind === "task" && detail.status !== "loading" && detail.data) ||
      kind === "diff" ||
      cockpit.data !== null;
    if (!contentReady) {
      return;
    }
    if (isNavigationPending()) {
      captureRouteVisible({ to_route: window.location.hash });
    }
    if (!pwaLaunchCapturedRef.current) {
      pwaLaunchCapturedRef.current = true;
      capturePwaLaunch();
    }
  }, [route, detail.status, detail.data, cockpit.data]);

  useEffect(() => {
    // Always consume: clears leftover enter class on button / bottom-nav navigations.
    setSwipeEnter(consumeSwipeEnterDirection());
  }, [route]);

  useEffect(() => {
    const node = outletSwipeRef.current;
    if (!node || !swipeEnter) return;
    const onAnimationEnd = (event: AnimationEvent) => {
      if (event.target !== node) return;
      setSwipeEnter(null);
    };
    node.addEventListener("animationend", onAnimationEnd);
    return () => node.removeEventListener("animationend", onAnimationEnd);
  }, [swipeEnter, route.kind]);

  const swipeOutletClass = swipeEnterClassName(swipeEnter);

  function reloadOnce() {
    if (reloadLatchRef.current) return;
    reloadLatchRef.current = true;
    void (async () => {
      try {
        if (await checkHealth()) {
          const { origin, hash } = window.location;
          if (origin.startsWith("http://") || origin.startsWith("https://")) {
            window.location.replace(`${origin}${hash}`);
            return;
          }
          location.reload();
          return;
        }
        await loadCockpit({ trailing: true });
      } catch {
        // Stay on the SPA; allow another tap.
      }
      // Keep the latch if we started a document navigation.
      reloadLatchRef.current = false;
    })();
  }

  const chrome = (
    <div className="cockpit-chrome">
      <header>
        <div className="bar">
          <h1>Ajax</h1>
          <p className="status-line" aria-live="polite">
            {statusText}
          </p>
          <button className="settings-link" type="button" onClick={() => go(settingsHash())}>
            Settings
          </button>
          <span
            className={`live-dot${connection === "connected" ? " is-live" : ""}`}
            aria-hidden="true"
          />
        </div>
        <ConnectionStatus
          state={connection}
          detail={connectionDetail}
          onRetry={() => void loadCockpit({ trailing: true })}
          onReload={reloadOnce}
          onCopyDiagnostics={() => go(settingsHash())}
        />
      </header>

      <div className="page-lead">
        <button
          className="update-banner"
          data-testid="update-banner"
          type="button"
          hidden={!updateAvailable}
          onClick={reloadOnce}
        >
          Update ready — tap to reload
        </button>
      </div>
    </div>
  );

  const isSessionRoute = route.kind === "session" && orchestrationChat;
  const hideBottomNav = isSessionRoute;

  const nav = hideBottomNav ? null : (
    <nav className="bottom-nav" aria-label="Mobile navigation">
      <button
        type="button"
        data-bottom-route="#/"
        aria-current={route.kind === "dashboard" || route.kind === "project" ? "page" : undefined}
        onClick={() => go(dashboardHash())}
      >
        Dashboard
      </button>
      <button
        type="button"
        data-bottom-action="new-task"
        // The New task sheet is the creator in both modes: it picks the harness
        // and its model, and starts provisioned when orchestration chat is on.
        onClick={() => setSheetOpen(true)}
      >
        New
      </button>
    </nav>
  );

  return (
    <AppViewport>
      <AppShell
        chrome={isSessionRoute ? null : chrome}
        nav={nav}
        className={isSessionRoute ? "app-shell--session" : undefined}
      >
        <RouteScroll>
          {route.kind === "settings" ? (
            <section data-outlet="settings" data-testid="outlet-settings" aria-live="polite">
              <SettingsView
                detailHandle={null}
                onResult={showResult}
                onBack={() => go(dashboardHash())}
                onRestarted={() => {
                  go(dashboardHash());
                  void loadCockpit();
                }}
              />
            </section>
          ) : route.kind === "diff" && route.handle ? (
            <section
              ref={outletSwipeRef}
              className={swipeOutletClass || undefined}
              data-outlet="diff"
              data-testid="outlet-diff"
              data-handle={route.handle}
              aria-live="polite"
            >
              {detail.status === "loading" ? (
                <Skeleton testid="task-skeleton" rows={6} />
              ) : detail.data ? (
                <DiffReview
                  handle={route.handle}
                  title={detail.data.title}
                  selectedPr={route.pr}
                  onBack={() => {
                    if (route.kind === "diff" && route.handle) {
                      const sessionCapable =
                        detail.data.session_capable !== false &&
                        cockpitRef.current.data?.cards?.some(
                          (card) =>
                            card.qualified_handle === route.handle && card.session_capable,
                        );
                      const terminalPreferred = readTaskTerminalPreferred(route.handle);
                      go(
                        orchestrationChat && sessionCapable && !terminalPreferred
                          ? sessionHash(route.handle)
                          : taskHash(route.handle),
                      );
                    }
                  }}
                  onSelectPr={(pr) => {
                    if (route.kind === "diff" && route.handle) {
                      go(taskDiffHash(route.handle, pr));
                    }
                  }}
                />
              ) : (
                <TaskLoadError
                  message={detail.error?.message ?? "Request failed"}
                  onRetry={reload}
                />
              )}
            </section>
          ) : route.kind === "session" && orchestrationChat ? (
            <section
              ref={outletSwipeRef}
              className={swipeOutletClass || undefined}
              data-outlet="session"
              data-testid="outlet-session"
              data-handle={route.handle}
              aria-live="polite"
            >
              {route.handle ? (
                <SessionChat
                  handle={route.handle}
                  detail={detail.data}
                  detailStatus={detail.status}
                  detailError={detail.error?.message}
                  onBack={() => go(selectedProject ? projectHash(selectedProject) : dashboardHash())}
                  onOpenDiff={() => route.handle && go(taskDiffHash(route.handle))}
                  onCockpit={applyCockpit}
                  onResult={showResult}
                  onSwappedAgent={() => {
                    if (route.kind === "session" && route.handle) clearSessionOutbox(route.handle);
                  }}
                  onOpenTerminal={() => {
                    if (route.handle) {
                      writeTaskTerminalPreferred(route.handle);
                      go(taskHash(route.handle));
                    }
                  }}
                  onMutated={() => route.kind === "session" && route.handle && reload()}
                  onDismiss={() => go(dashboardHash())}
                  onRetry={reload}
                  pendingConfirmAction={pendingConfirm?.action.action ?? null}
                  onCancelPendingConfirm={cancelPendingConfirm}
                />
              ) : (
                <NewTaskSheet
                  repos={cockpit.data?.repos?.repos ?? []}
                  selectedProject={selectedProject}
                  onClose={() => go(dashboardHash())}
                  onCockpit={applyCockpit}
                  onOpenTask={(handle, latestCockpit) => openTask(handle, latestCockpit)}
                />
              )}
            </section>
          ) : route.kind === "task" ? (
            <section
              ref={outletSwipeRef}
              className={swipeOutletClass || undefined}
              data-outlet="task"
              data-testid="outlet-task"
              data-handle={route.handle}
              aria-live="polite"
            >
              {detail.status === "loading" ? (
                <Skeleton testid="task-skeleton" rows={6} />
              ) : detail.data ? (
                <TaskDetail
                  detail={detail.data}
                  orchestrationChat={orchestrationChat}
                  onBack={() => go(selectedProject ? projectHash(selectedProject) : dashboardHash())}
                  onOpenDiff={() => route.handle && go(taskDiffHash(route.handle))}
                  onOpenChat={() => {
                    if (route.handle) {
                      clearTaskTerminalPreferred(route.handle);
                      go(sessionHash(route.handle));
                    }
                  }}
                  onCockpit={applyCockpit}
                  onResult={showResult}
                  onMutated={() => route.kind === "task" && route.handle && reload()}
                  onDismiss={() => go(dashboardHash())}
                  pendingConfirmAction={pendingConfirm?.action.action ?? null}
                  onCancelPendingConfirm={cancelPendingConfirm}
                />
              ) : (
                <TaskLoadError
                  message={detail.error?.message ?? "Request failed"}
                  onRetry={reload}
                />
              )}
            </section>
          ) : (
            <section
              ref={(node) => {
                pullToRefreshRef(node);
                outletSwipeRef.current = node;
              }}
              className={swipeOutletClass || undefined}
              data-outlet={route.kind === "project" ? "project" : "dashboard"}
              data-testid={route.kind === "project" ? "outlet-project" : "outlet-dashboard"}
              aria-live="polite"
            >
              <div
                className={`pull-indicator${pullDistance >= PULL_THRESHOLD ? " armed" : ""}`}
                style={{ height: `${pullDistance}px` }}
                aria-hidden="true"
              >
                <span className="pull-spinner" />
              </div>
              {cockpit.data ? (
                <TaskList
                  cockpit={cockpit.data}
                  selectedProject={selectedProject}
                  onSelectProject={(project: string | null) =>
                    go(project ? projectHash(project) : dashboardHash())
                  }
                  onOpenTask={openTask}
                  onCockpit={applyCockpit}
                  onResult={showResult}
                  onMutated={() => loadCockpit()}
                  pendingConfirmAction={pendingConfirm?.action.action ?? null}
                  onCancelPendingConfirm={cancelPendingConfirm}
                />
              ) : (
                <Skeleton testid="dashboard-skeleton" rows={4} />
              )}
            </section>
          )}
        </RouteScroll>
      </AppShell>

      {pendingConfirm ? (
        <ResultPanel
          message={`Confirm ${pendingConfirm.action.label} for ${pendingConfirm.handle}?`}
          onConfirm={commitPendingConfirm}
          onCancelConfirm={cancelPendingConfirm}
          onConfirmTimeout={expirePendingConfirm}
          onDismiss={dismissPendingConfirm}
        />
      ) : null}

      {result && !pendingConfirm ? (
        <ResultPanel
          message={result.message}
          output={result.output}
          isError={result.isError}
          onUndo={result.onUndo}
          onCommit={result.onCommit}
          onDismiss={() => setResult(null)}
        />
      ) : null}

      {sheetOpen && sheetAllowed && (
        <NewTaskSheet
          repos={cockpit.data?.repos?.repos ?? []}
          selectedProject={selectedProject}
          onClose={() => setSheetOpen(false)}
          onCockpit={applyCockpit}
          onOpenTask={openTask}
        />
      )}
    </AppViewport>
  );
}
