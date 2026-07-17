<script lang="ts">
  import { untrack } from "svelte";
  import { parseRoute, dashboardHash, settingsHash, taskHash, projectHash, type Route } from "../routes";
  import type { BrowserCockpitView, BrowserTaskDetail, ConnectionState } from "../types";
  import { ApiError, fetchCockpit, fetchDetail, fetchVersion, postOperation, requestId } from "../api";
  import {
    cockpitRefreshIntervalMs,
    versionPollIntervalMs,
    type PollingRouteKind,
  } from "../polling";
  import ReactIsland from "../react/ReactIsland.svelte";
  import ConnectionStatus from "./ConnectionStatus";
  import ResultPanel from "./ResultPanel.svelte";
  import TaskList from "./TaskList";
  import TaskDetail from "./TaskDetail.svelte";
  import SettingsView from "./SettingsView.svelte";
  import NewTaskSheet from "./NewTaskSheet.svelte";
  import Skeleton from "./Skeleton";
  import AppViewport from "./AppViewport.svelte";
  import AppShell from "./AppShell.svelte";
  import RouteScroll from "./RouteScroll.svelte";
  import { pullToRefresh } from "../gestures/pullToRefreshAction";
  import { PULL_THRESHOLD } from "../gestures/pullToRefresh";
  import { createCockpitApplyGate, createInFlightGuard } from "../cockpitPoll";

  // Shallow, replaceable projection of server truth — never an authored store.
  let route = $state<Route>(parseRoute(typeof location !== "undefined" ? location.hash : ""));
  let cockpit = $state<BrowserCockpitView | null>(null);
  let detail = $state<BrowserTaskDetail | null>(null);
  let connection = $state<ConnectionState>("checking");
  let connectionDetail = $state<string | null>(null);
  let updateAvailable = $state(false);
  let sheetOpen = $state(false);
  let result = $state<{
    message: string;
    output?: string | null;
    isError: boolean;
    onUndo?: () => void;
    onCommit?: () => void;
  } | null>(null);
  let pullDistance = $state(0);
  let documentVisibility = $state<DocumentVisibilityState>(
    typeof document !== "undefined" ? document.visibilityState : "visible",
  );

  let selectedProject = $derived(route.kind === "project" ? (route.project ?? null) : null);
  let taskOpenHandle = $derived(route.kind === "task" ? route.handle : null);
  let bootVersion: string | null = null;

  const cockpitApplyGate = createCockpitApplyGate();
  const cockpitPollGuard = createInFlightGuard();

  function showResult(
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: { onUndo?: () => void; onCommit?: () => void },
  ) {
    result = { message, output, isError, onUndo: options?.onUndo, onCommit: options?.onCommit };
  }

  function applyCockpit(next: BrowserCockpitView) {
    if (cockpitApplyGate.applyIfChanged(next)) {
      cockpit = next;
    }
    connection = "connected";
    connectionDetail = null;
  }

  function applyConnectionError(error: unknown) {
    if (error instanceof ApiError) {
      connection =
        error.kind === "network"
          ? "backend unreachable"
          : error.kind === "stale-session"
            ? "stale session"
            : "disconnected";
      connectionDetail = error.message;
      return;
    }
    connection = "backend unreachable";
    connectionDetail = error instanceof Error ? error.message : String(error);
  }

  async function loadCockpit() {
    if (document.hidden) return;
    await cockpitPollGuard.run(async () => {
      try {
        applyCockpit(await fetchCockpit());
      } catch (error) {
        applyConnectionError(error);
      }
    });
  }

  // Opening a task IS the resume gesture (matches TUI Enter): dispatch the
  // operator action so core acknowledges attention and the inbox row clears.
  // Best-effort — a blocked/failed resume is not a connection error; the detail
  // projection already carries the status explanation.
  async function resumeOnOpen(handle: string): Promise<boolean> {
    try {
      const result = await postOperation({
        task_handle: handle,
        action: "resume",
        request_id: requestId(),
      });
      if (result.ok && result.response.cockpit) applyCockpit(result.response.cockpit);
      return result.ok;
    } catch {
      // swallow: resume is not required for viewing.
      return false;
    }
  }

  async function loadDetail(handle: string) {
    try {
      const next = await fetchDetail(handle);
      // Stale response: the user already navigated to another task (or away).
      if (taskOpenHandle !== handle) return;
      detail = next;
      connection = "connected";
      connectionDetail = null;
    } catch (error) {
      if (error instanceof ApiError) {
        applyConnectionError(error);
      }
    }
  }

  async function checkVersion() {
    try {
      const { version } = await fetchVersion();
      if (!version) return;
      if (bootVersion === null) bootVersion = version;
      else if (version !== bootVersion) updateAvailable = true;
    } catch {
      // Offline: keep the pinned version and retry later.
    }
  }

  // Run non-critical boot work once the browser is idle so it never blocks
  // first paint. Falls back to a near-immediate timer where unsupported (iOS).
  function whenIdle(callback: () => void): number {
    if (typeof requestIdleCallback === "function") return requestIdleCallback(callback);
    return setTimeout(callback, 1) as unknown as number;
  }

  function cancelIdle(handle: number) {
    if (typeof cancelIdleCallback === "function") cancelIdleCallback(handle);
    else clearTimeout(handle);
  }

  // Shell listeners — mount once; immediate poll on focus / pageshow / become-visible.
  $effect(() => {
    void loadCockpit();
    const idleHandle = whenIdle(() => void checkVersion());
    const onHashChange = () => (route = parseRoute(location.hash));
    const onResume = () => {
      void checkVersion();
      void loadCockpit();
    };
    const onVisibilityChange = () => {
      documentVisibility = document.visibilityState;
      if (document.visibilityState === "visible") {
        void checkVersion();
        void loadCockpit();
      }
    };
    window.addEventListener("hashchange", onHashChange);
    window.addEventListener("focus", onResume);
    window.addEventListener("pageshow", onResume);
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      cancelIdle(idleHandle);
      window.removeEventListener("hashchange", onHashChange);
      window.removeEventListener("focus", onResume);
      window.removeEventListener("pageshow", onResume);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  });

  // Adaptive cockpit / version intervals — reschedule on route or visibility change.
  $effect(() => {
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
  });

  // Detail loading — re-run only when the selected task handle changes.
  $effect(() => {
    const handle = taskOpenHandle;
    if (!handle) {
      detail = null;
      return;
    }
    untrack(() => {
      detail = null;
      void loadDetail(handle);
      void resumeOnOpen(handle).then((mutated) => {
        if (mutated) void loadDetail(handle);
      });
    });
  });

  function go(hash: string) {
    location.hash = hash;
  }

  let statusText = $derived(
    cockpit
      ? `${cockpit.cards.length} ${cockpit.cards.length === 1 ? "task" : "tasks"}`
      : "— loading",
  );

  $effect(() => {
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
  });
</script>

<AppViewport>
  <AppShell>
    {#snippet chrome()}
      <div class="cockpit-chrome">
        <header>
          <div class="bar">
            <h1>Ajax</h1>
            <p class="status-line" aria-live="polite">{statusText}</p>
            <button class="settings-link" type="button" onclick={() => go(settingsHash())}>Settings</button>
            <span
              class="live-dot"
              class:is-live={connection === "connected"}
              aria-hidden="true"
            ></span>
          </div>
          <ReactIsland
            component={ConnectionStatus}
            props={{
              state: connection,
              detail: connectionDetail,
              onRetry: () => loadCockpit(),
              onReload: () => location.reload(),
              onCopyDiagnostics: () => go(settingsHash()),
            }}
          />
        </header>

        <div class="page-lead">
          <button
            class="update-banner"
            type="button"
            hidden={!updateAvailable}
            onclick={() => location.reload()}
          >
            Update ready — tap to reload
          </button>
        </div>
      </div>
    {/snippet}

    {#snippet children()}
      <RouteScroll>
        {#if route.kind === "settings"}
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
        {:else if route.kind === "task"}
          <section data-outlet="task" data-testid="outlet-task" data-handle={route.handle} aria-live="polite">
            {#if detail}
              <TaskDetail
                {detail}
                onBack={() => go(selectedProject ? projectHash(selectedProject) : dashboardHash())}
                onCockpit={applyCockpit}
                onResult={showResult}
                onMutated={() => route.kind === "task" && route.handle && loadDetail(route.handle)}
                onDismiss={() => go(dashboardHash())}
              />
            {:else}
              <ReactIsland component={Skeleton} props={{ testid: "task-skeleton", rows: 6 }} />
            {/if}
          </section>
        {:else}
          <section
            data-outlet={route.kind === "project" ? "project" : "dashboard"}
            data-testid={route.kind === "project" ? "outlet-project" : "outlet-dashboard"}
            aria-live="polite"
            use:pullToRefresh={{ onRefresh: () => loadCockpit(), onDistance: (d) => (pullDistance = d) }}
          >
            <div
              class="pull-indicator"
              class:armed={pullDistance >= PULL_THRESHOLD}
              style="height: {pullDistance}px"
              aria-hidden="true"
            >
              <span class="pull-spinner"></span>
            </div>
            {#if cockpit}
              <ReactIsland
                component={TaskList}
                props={{
                  cockpit,
                  selectedProject,
                  onSelectProject: (project: string | null) =>
                    go(project ? projectHash(project) : dashboardHash()),
                  onOpenTask: (handle: string) => go(taskHash(handle)),
                  onCockpit: applyCockpit,
                  onResult: showResult,
                  onMutated: () => loadCockpit(),
                }}
              />
            {:else}
              <ReactIsland component={Skeleton} props={{ testid: "dashboard-skeleton", rows: 4 }} />
            {/if}
          </section>
        {/if}
      </RouteScroll>
    {/snippet}

    {#snippet nav()}
      <nav class="bottom-nav" aria-label="Mobile navigation">
        <button
          type="button"
          data-bottom-route="#/"
          aria-current={route.kind === "dashboard" || route.kind === "project" ? "page" : undefined}
          onclick={() => go(dashboardHash())}
        >
          Dashboard
        </button>
        <button type="button" data-bottom-action="new-task" onclick={() => (sheetOpen = true)}>New</button>
      </nav>
    {/snippet}
  </AppShell>

  {#if result}
    <ResultPanel
      message={result.message}
      output={result.output}
      isError={result.isError}
      onUndo={result.onUndo}
      onCommit={result.onCommit}
      onDismiss={() => (result = null)}
    />
  {/if}

  {#if sheetOpen}
    <NewTaskSheet
      repos={cockpit?.repos?.repos ?? []}
      {selectedProject}
      onClose={() => (sheetOpen = false)}
      onCockpit={applyCockpit}
      onResult={showResult}
      onOpenTask={(handle) => go(taskHash(handle))}
    />
  {/if}
</AppViewport>

<style>
  /* PULL-TO-REFRESH INDICATOR — height is driven by the gesture distance. */
  .pull-indicator {
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    margin-bottom: 0;
  }

  .pull-spinner {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid var(--rule-strong);
    border-top-color: var(--accent);
    opacity: 0.5;
    transition: opacity 140ms var(--ease), transform 140ms var(--ease);
  }

  .pull-indicator.armed .pull-spinner {
    opacity: 1;
    transform: rotate(180deg);
  }
</style>
