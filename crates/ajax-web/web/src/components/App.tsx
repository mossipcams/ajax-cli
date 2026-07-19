import { useCallback, useEffect, useRef, useState } from "react";
import { dashboardHash, projectHash, settingsHash, taskHash } from "../routes";
import type { BrowserCockpitView, BrowserTaskDetail, ConnectionState } from "../types";
import { ApiError, fetchCockpit, fetchDetail, fetchVersion, postOperation, requestId } from "../api";
import {
  cockpitRefreshIntervalMs,
  versionPollIntervalMs,
  type PollingRouteKind,
} from "../polling";
import ConnectionStatus from "./ConnectionStatus";
import ResultPanel from "./ResultPanel";
import TaskList from "./TaskList";
import TaskDetail from "./TaskDetail";
import SettingsView from "./SettingsView";
import NewTaskSheet from "./NewTaskSheet";
import Skeleton from "./Skeleton";
import AppViewport from "./AppViewport";
import AppShell from "./AppShell";
import RouteScroll from "./RouteScroll";
import { PULL_THRESHOLD } from "../gestures/pullToRefresh";
import { createCockpitApplyGate, createInFlightGuard } from "../cockpitPoll";
import { useHashRoute } from "../react/useHashRoute";
import { usePullToRefresh } from "../react/usePullToRefresh";

type ResultState = {
  message: string;
  output?: string | null;
  isError: boolean;
  onUndo?: () => void;
  onCommit?: () => void;
};

export default function App() {
  const route = useHashRoute();
  const [cockpit, setCockpit] = useState<BrowserCockpitView | null>(null);
  const [detail, setDetail] = useState<BrowserTaskDetail | null>(null);
  const [connection, setConnection] = useState<ConnectionState>("checking");
  const [connectionDetail, setConnectionDetail] = useState<string | null>(null);
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [result, setResult] = useState<ResultState | null>(null);
  const [pullDistance, setPullDistance] = useState(0);
  const [documentVisibility, setDocumentVisibility] = useState<DocumentVisibilityState>(
    typeof document !== "undefined" ? document.visibilityState : "visible",
  );

  const bootVersionRef = useRef<string | null>(null);
  const cockpitApplyGateRef = useRef(createCockpitApplyGate());
  const cockpitPollGuardRef = useRef(createInFlightGuard());

  const selectedProject = route.kind === "project" ? (route.project ?? null) : null;
  const taskOpenHandle = route.kind === "task" ? route.handle : null;
  const taskOpenHandleRef = useRef(taskOpenHandle);
  taskOpenHandleRef.current = taskOpenHandle;

  const statusText = cockpit
    ? `${cockpit.cards.length} ${cockpit.cards.length === 1 ? "task" : "tasks"}`
    : "— loading";

  function showResult(
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: { onUndo?: () => void; onCommit?: () => void },
  ) {
    setResult({ message, output, isError, onUndo: options?.onUndo, onCommit: options?.onCommit });
  }

  const applyCockpit = useCallback((next: BrowserCockpitView) => {
    if (cockpitApplyGateRef.current.applyIfChanged(next)) {
      setCockpit(next);
    }
    setConnection("connected");
    setConnectionDetail(null);
  }, []);

  const applyConnectionError = useCallback((error: unknown) => {
    if (error instanceof ApiError) {
      setConnection(
        error.kind === "network"
          ? "backend unreachable"
          : error.kind === "stale-session"
            ? "stale session"
            : "disconnected",
      );
      setConnectionDetail(error.message);
      return;
    }
    setConnection("backend unreachable");
    setConnectionDetail(error instanceof Error ? error.message : String(error));
  }, []);

  const loadCockpit = useCallback(async () => {
    if (document.hidden) return;
    await cockpitPollGuardRef.current.run(async () => {
      try {
        applyCockpit(await fetchCockpit());
      } catch (error) {
        applyConnectionError(error);
      }
    });
  }, [applyCockpit, applyConnectionError]);

  const resumeOnOpen = useCallback(
    async (handle: string): Promise<boolean> => {
      try {
        const opResult = await postOperation({
          task_handle: handle,
          action: "resume",
          request_id: requestId(),
        });
        if (opResult.ok && opResult.response.cockpit) applyCockpit(opResult.response.cockpit);
        return opResult.ok;
      } catch {
        return false;
      }
    },
    [applyCockpit],
  );

  const loadDetail = useCallback(
    async (handle: string) => {
      try {
        const next = await fetchDetail(handle);
        if (taskOpenHandleRef.current !== handle) return;
        setDetail(next);
        setConnection("connected");
        setConnectionDetail(null);
      } catch (error) {
        if (error instanceof ApiError) {
          applyConnectionError(error);
        }
      }
    },
    [applyConnectionError],
  );

  const checkVersion = useCallback(async () => {
    try {
      const { version } = await fetchVersion();
      if (!version) return;
      if (bootVersionRef.current === null) bootVersionRef.current = version;
      else if (version !== bootVersionRef.current) setUpdateAvailable(true);
    } catch {
      // Offline: keep the pinned version and retry later.
    }
  }, []);

  function whenIdle(callback: () => void): number {
    if (typeof requestIdleCallback === "function") return requestIdleCallback(callback);
    return setTimeout(callback, 1) as unknown as number;
  }

  function cancelIdle(handle: number) {
    if (typeof cancelIdleCallback === "function") cancelIdleCallback(handle);
    else clearTimeout(handle);
  }

  function go(hash: string) {
    location.hash = hash;
  }

  const pullToRefreshRef = usePullToRefresh({
    onRefresh: () => loadCockpit(),
    onDistance: setPullDistance,
  });

  // Shell listeners — mount once; immediate poll on focus / pageshow / become-visible.
  useEffect(() => {
    void loadCockpit();
    const idleHandle = whenIdle(() => void checkVersion());
    const onResume = () => {
      void checkVersion();
      void loadCockpit();
    };
    const onVisibilityChange = () => {
      setDocumentVisibility(document.visibilityState);
      if (document.visibilityState === "visible") {
        void checkVersion();
        void loadCockpit();
      }
    };
    window.addEventListener("focus", onResume);
    window.addEventListener("pageshow", onResume);
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      cancelIdle(idleHandle);
      window.removeEventListener("focus", onResume);
      window.removeEventListener("pageshow", onResume);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
    // ponytail: mount-once shell listeners; loadCockpit/checkVersion are stable enough via refs/guards
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Adaptive cockpit / version intervals — reschedule on route or visibility change.
  useEffect(() => {
    const input = {
      visibilityState: documentVisibility,
      routeKind: route.kind as PollingRouteKind,
    };
    const cockpitTimer = setInterval(loadCockpit, cockpitRefreshIntervalMs(input));
    const versionTimer = setInterval(checkVersion, versionPollIntervalMs(input));
    return () => {
      clearInterval(cockpitTimer);
      clearInterval(versionTimer);
    };
    // ponytail: loadCockpit/checkVersion intentionally omitted; they are stable callbacks
    // and re-running this effect on their identity would churn both intervals
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [documentVisibility, route.kind]);

  // Detail loading — re-run only when the selected task handle changes.
  useEffect(() => {
    const handle = taskOpenHandle;
    if (!handle) {
      setDetail(null);
      return;
    }
    setDetail(null);
    void loadDetail(handle);
    void resumeOnOpen(handle).then((mutated) => {
      if (mutated) void loadDetail(handle);
    });
  }, [taskOpenHandle, loadDetail, resumeOnOpen]);

  useEffect(() => {
    const kind = route.kind;
    if (kind === "task" && route.handle) {
      document.title = `${route.handle} — Ajax`;
    } else if (kind === "settings") {
      document.title = "Settings — Ajax";
    } else if (kind === "project" && route.project) {
      document.title = `${route.project} — Ajax`;
    } else {
      document.title = "Ajax";
    }
  }, [route]);

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
          onRetry={() => loadCockpit()}
          onReload={() => location.reload()}
          onCopyDiagnostics={() => go(settingsHash())}
        />
      </header>

      <div className="page-lead">
        <button
          className="update-banner"
          type="button"
          hidden={!updateAvailable}
          onClick={() => location.reload()}
        >
          Update ready — tap to reload
        </button>
      </div>
    </div>
  );

  const nav = (
    <nav className="bottom-nav" aria-label="Mobile navigation">
      <button
        type="button"
        data-bottom-route="#/"
        aria-current={route.kind === "dashboard" || route.kind === "project" ? "page" : undefined}
        onClick={() => go(dashboardHash())}
      >
        Dashboard
      </button>
      <button type="button" data-bottom-action="new-task" onClick={() => setSheetOpen(true)}>
        New
      </button>
    </nav>
  );

  return (
    <AppViewport>
      <AppShell chrome={chrome} nav={nav}>
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
          ) : route.kind === "task" ? (
            <section
              data-outlet="task"
              data-testid="outlet-task"
              data-handle={route.handle}
              aria-live="polite"
            >
              {detail ? (
                <TaskDetail
                  detail={detail}
                  onBack={() => go(selectedProject ? projectHash(selectedProject) : dashboardHash())}
                  onCockpit={applyCockpit}
                  onResult={showResult}
                  onMutated={() => route.kind === "task" && route.handle && loadDetail(route.handle)}
                  onDismiss={() => go(dashboardHash())}
                />
              ) : (
                <Skeleton testid="task-skeleton" rows={6} />
              )}
            </section>
          ) : (
            <section
              ref={pullToRefreshRef}
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
              {cockpit ? (
                <TaskList
                  cockpit={cockpit}
                  selectedProject={selectedProject}
                  onSelectProject={(project: string | null) =>
                    go(project ? projectHash(project) : dashboardHash())
                  }
                  onOpenTask={(handle: string) => go(taskHash(handle))}
                  onCockpit={applyCockpit}
                  onResult={showResult}
                  onMutated={() => loadCockpit()}
                />
              ) : (
                <Skeleton testid="dashboard-skeleton" rows={4} />
              )}
            </section>
          )}
        </RouteScroll>
      </AppShell>

      {result && (
        <ResultPanel
          message={result.message}
          output={result.output}
          isError={result.isError}
          onUndo={result.onUndo}
          onCommit={result.onCommit}
          onDismiss={() => setResult(null)}
        />
      )}

      {sheetOpen && (
        <NewTaskSheet
          repos={cockpit?.repos?.repos ?? []}
          selectedProject={selectedProject}
          onClose={() => setSheetOpen(false)}
          onCockpit={applyCockpit}
          onResult={showResult}
          onOpenTask={(handle: string) => go(taskHash(handle))}
        />
      )}
    </AppViewport>
  );
}
