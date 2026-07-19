import { useEffect, useEffectEvent, useState } from "react";
import { dashboardHash, projectHash, settingsHash, taskHash } from "@/shared/lib/routes";
import {
  cockpitRefreshIntervalMs,
  versionPollIntervalMs,
  type PollingRouteKind,
} from "@/shared/lib/polling";
import ConnectionStatus from "@/shared/ui/ConnectionStatus";
import ResultPanel from "@/shared/ui/ResultPanel";
import TaskList from "@/features/task/TaskList";
import TaskDetail from "@/features/task/TaskDetail";
import TaskLoadError from "@/features/task/TaskLoadError";
import SettingsView from "@/features/settings/SettingsView";
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

type ResultState = {
  message: string;
  output?: string | null;
  isError: boolean;
  onUndo?: () => void;
  onCommit?: () => void;
};

export default function App() {
  const route = useHashRoute();
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
  const taskOpenHandle = route.kind === "task" ? (route.handle ?? null) : null;
  const { detail, reload } = useTaskDetailResource(taskOpenHandle, {
    applyCockpit,
    applyConnectionError,
    markConnected,
  });
  const { updateAvailable, checkVersion } = useVersionMonitor();
  const [sheetOpen, setSheetOpen] = useState(false);
  const [result, setResult] = useState<ResultState | null>(null);
  const [pullDistance, setPullDistance] = useState(0);
  const [documentVisibility, setDocumentVisibility] = useState<DocumentVisibilityState>(
    typeof document !== "undefined" ? document.visibilityState : "visible",
  );

  const statusText = cockpit.data
    ? `${cockpit.data.cards.length} ${cockpit.data.cards.length === 1 ? "task" : "tasks"}`
    : "— loading";

  function showResult(
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: { onUndo?: () => void; onCommit?: () => void },
  ) {
    setResult({ message, output, isError, onUndo: options?.onUndo, onCommit: options?.onCommit });
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
    location.hash = hash;
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
  const onShellResume = useEffectEvent(() => {
    void checkVersion();
    void loadCockpit();
  });
  const onShellVisibilityChange = useEffectEvent(() => {
    setDocumentVisibility(document.visibilityState);
    if (document.visibilityState === "visible") {
      void checkVersion();
      void loadCockpit();
    }
  });

  // Shell listeners — mount once; immediate poll on focus / pageshow / become-visible.
  useEffect(() => {
    const idleHandle = onShellMount();
    const onResume = () => onShellResume();
    const onVisibilityChange = () => onShellVisibilityChange();
    window.addEventListener("focus", onResume);
    window.addEventListener("pageshow", onResume);
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      cancelIdle(idleHandle);
      window.removeEventListener("focus", onResume);
      window.removeEventListener("pageshow", onResume);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);

  // Adaptive cockpit / version intervals. Derive the scalar cadences first: an
  // inline object literal is a new value every render and could never be a
  // dependency, which is what forced the old suppression here.
  const pollingInput = {
    visibilityState: documentVisibility,
    routeKind: route.kind as PollingRouteKind,
  };
  const cockpitIntervalMs = cockpitRefreshIntervalMs(pollingInput);
  const versionIntervalMs = versionPollIntervalMs(pollingInput);

  useEffect(() => {
    const cockpitTimer = window.setInterval(loadCockpit, cockpitIntervalMs);
    const versionTimer = window.setInterval(checkVersion, versionIntervalMs);
    return () => {
      window.clearInterval(cockpitTimer);
      window.clearInterval(versionTimer);
    };
  }, [checkVersion, cockpitIntervalMs, loadCockpit, versionIntervalMs]);

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
          data-testid="update-banner"
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
              {detail.status === "loading" ? (
                <Skeleton testid="task-skeleton" rows={6} />
              ) : detail.data ? (
                <TaskDetail
                  detail={detail.data}
                  onBack={() => go(selectedProject ? projectHash(selectedProject) : dashboardHash())}
                  onCockpit={applyCockpit}
                  onResult={showResult}
                  onMutated={() => route.kind === "task" && route.handle && reload()}
                  onDismiss={() => go(dashboardHash())}
                />
              ) : (
                <TaskLoadError message={detail.error.message} onRetry={reload} />
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
              {cockpit.data ? (
                <TaskList
                  cockpit={cockpit.data}
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
          repos={cockpit.data?.repos?.repos ?? []}
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
