<script lang="ts">
  import { untrack } from "svelte";
  import { parseRoute, dashboardHash, settingsHash, taskHash, projectHash, type Route } from "../routes";
  import type { BrowserCockpitView, BrowserTaskDetail, ConnectionState } from "../types";
  import { ApiError, fetchCockpit, fetchDetail, fetchVersion } from "../api";
  import { REFRESH_INTERVAL_MS, VERSION_POLL_MS } from "../polling";
  import { unregisterExistingServiceWorkers } from "../diagnostics";
  import ConnectionStatus from "./ConnectionStatus.svelte";
  import ResultPanel from "./ResultPanel.svelte";
  import TaskList from "./TaskList.svelte";
  import TaskDetail from "./TaskDetail.svelte";
  import SettingsView from "./SettingsView.svelte";
  import NewTaskSheet from "./NewTaskSheet.svelte";

  // Shallow, replaceable projection of server truth — never an authored store.
  let route = $state<Route>(parseRoute(typeof location !== "undefined" ? location.hash : ""));
  let cockpit = $state<BrowserCockpitView | null>(null);
  let detail = $state<BrowserTaskDetail | null>(null);
  let connection = $state<ConnectionState>("checking");
  let connectionDetail = $state<string | null>(null);
  let updateAvailable = $state(false);
  let sheetOpen = $state(false);
  let result = $state<{ message: string; output?: string | null; isError: boolean } | null>(null);

  let selectedProject = $derived(route.kind === "project" ? (route.project ?? null) : null);
  let bootVersion: string | null = null;

  function showResult(message: string, output: string | null | undefined, isError: boolean) {
    result = { message, output, isError };
  }

  function applyCockpit(next: BrowserCockpitView) {
    cockpit = next;
    connection = "connected";
  }

  async function loadCockpit() {
    if (document.hidden) return;
    try {
      applyCockpit(await fetchCockpit());
    } catch {
      connection = "backend unreachable";
    }
  }

  async function loadDetail(handle: string) {
    try {
      detail = await fetchDetail(handle);
      connection = "connected";
    } catch (error) {
      if (error instanceof ApiError && error.kind === "network") {
        connection = "backend unreachable";
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

  // Cockpit polling — mount once; the interval callback is not a tracked read.
  $effect(() => {
    unregisterExistingServiceWorkers();
    void loadCockpit();
    void checkVersion();
    const cockpitTimer = setInterval(loadCockpit, REFRESH_INTERVAL_MS);
    const versionTimer = setInterval(checkVersion, VERSION_POLL_MS);
    const onHashChange = () => (route = parseRoute(location.hash));
    const onResume = () => {
      void checkVersion();
      void loadCockpit();
    };
    window.addEventListener("hashchange", onHashChange);
    window.addEventListener("focus", onResume);
    window.addEventListener("pageshow", onResume);
    document.addEventListener("visibilitychange", onResume);
    return () => {
      clearInterval(cockpitTimer);
      clearInterval(versionTimer);
      window.removeEventListener("hashchange", onHashChange);
      window.removeEventListener("focus", onResume);
      window.removeEventListener("pageshow", onResume);
      document.removeEventListener("visibilitychange", onResume);
    };
  });

  // Detail loading — re-run only when the selected task handle changes.
  $effect(() => {
    const handle = route.kind === "task" ? route.handle : null;
    if (!handle) {
      detail = null;
      return;
    }
    untrack(() => {
      detail = null;
      void loadDetail(handle);
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
</script>

<div class="cockpit-chrome">
  {#if result}
    <ResultPanel
      message={result.message}
      output={result.output}
      isError={result.isError}
      onDismiss={() => (result = null)}
    />
  {/if}

  <header>
    <div class="bar">
      <h1>Ajax</h1>
      <p class="status-line" aria-live="polite">{statusText}</p>
      <button class="settings-link" type="button" onclick={() => go(settingsHash())}>Settings</button>
      <span class="live-dot" aria-hidden="true"></span>
    </div>
    <ConnectionStatus
      state={connection}
      detail={connectionDetail}
      onRetry={() => loadCockpit()}
      onReload={() => location.reload()}
      onCopyDiagnostics={() => go(settingsHash())}
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

<main>
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
        />
      {:else}
        <p class="empty">Loading task…</p>
      {/if}
    </section>
  {:else}
    <section
      data-outlet={route.kind === "project" ? "project" : "dashboard"}
      data-testid={route.kind === "project" ? "outlet-project" : "outlet-dashboard"}
      aria-live="polite"
    >
      {#if cockpit}
        <TaskList
          {cockpit}
          {selectedProject}
          onSelectProject={(project) => go(project ? projectHash(project) : dashboardHash())}
          onOpenTask={(handle) => go(taskHash(handle))}
          onCockpit={applyCockpit}
          onResult={showResult}
          onMutated={() => loadCockpit()}
        />
      {:else}
        <p class="empty">— loading</p>
      {/if}
      <button
        class="new-task-row"
        type="button"
        data-bottom-action="new-task"
        onclick={() => (sheetOpen = true)}
      >
        <span class="new-task-glyph" aria-hidden="true">+</span>
        <span class="new-task-label">{selectedProject ? `New task in ${selectedProject}` : "New task"}</span>
      </button>
    </section>
  {/if}
</main>

<nav class="bottom-nav" aria-label="Mobile navigation">
  <button type="button" data-bottom-route="#/" onclick={() => go(dashboardHash())}>Dashboard</button>
  <button type="button" data-bottom-action="new-task" onclick={() => (sheetOpen = true)}>New</button>
</nav>

{#if sheetOpen}
  <NewTaskSheet
    repos={cockpit?.repos?.repos ?? []}
    {selectedProject}
    onClose={() => (sheetOpen = false)}
    onCockpit={applyCockpit}
    onResult={showResult}
  />
{/if}

<style>
  /* NEW TASK ROW — dashed call-to-action below the calm task list. */
  .new-task-row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    margin-top: 16px;
    padding: 13px 16px;
    background: transparent;
    border: 1px dashed var(--rule-strong);
    border-radius: var(--radius);
    color: var(--ink-soft);
    text-align: left;
    transition: background 140ms ease, border-color 140ms ease, color 140ms ease;
  }

  .new-task-row:hover,
  .new-task-row:focus-visible {
    background: var(--paper-tint);
    border-color: var(--teal-bright);
    color: var(--ink);
    outline: none;
  }

  .new-task-glyph {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--teal);
    color: var(--ink);
    font-size: 17px;
    font-weight: 600;
    line-height: 1;
  }

  .new-task-label {
    flex: 1 1 auto;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
</style>
