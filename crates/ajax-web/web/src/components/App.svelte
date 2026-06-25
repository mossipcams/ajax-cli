<script lang="ts">
  import { parseRoute, dashboardHash, settingsHash, type Route } from "../routes";
  import type { ConnectionState } from "../types";
  import ConnectionStatus from "./ConnectionStatus.svelte";
  import ResultPanel from "./ResultPanel.svelte";

  // Shallow, replaceable UI state only — no task truth lives here.
  let route = $state<Route>(parseRoute(typeof location !== "undefined" ? location.hash : ""));
  let connection = $state<ConnectionState>("checking");
  let connectionDetail = $state<string | null>(null);
  let statusText = $state("— loading");
  let updateAvailable = $state(false);
  let result = $state<{ message: string; output?: string | null; isError: boolean } | null>(null);

  // Task-specific view state is discarded whenever the route leaves detail, so a
  // stale pane buffer can never bleed across task selections.
  let activeTaskHandle = $state<string | null>(null);
  $effect(() => {
    activeTaskHandle = route.kind === "task" ? (route.handle ?? null) : null;
  });

  $effect(() => {
    const onHashChange = () => {
      route = parseRoute(location.hash);
    };
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  });

  function go(hash: string) {
    location.hash = hash;
  }
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
      onRetry={() => {}}
      onReload={() => location.reload()}
      onCopyDiagnostics={() => {}}
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
      <!-- SettingsView mounts here in Phase 4.6 -->
    </section>
  {:else if route.kind === "task"}
    <section data-outlet="task" data-testid="outlet-task" data-handle={activeTaskHandle} aria-live="polite">
      <!-- TaskDetail + PanePanel mount here in Phase 4.4/4.5 -->
    </section>
  {:else if route.kind === "project"}
    <section data-outlet="project" data-testid="outlet-project" data-project={route.project} aria-live="polite">
      <!-- TaskList (filtered) mounts here in Phase 4.1 -->
    </section>
  {:else}
    <section data-outlet="dashboard" data-testid="outlet-dashboard" aria-live="polite">
      <!-- TaskList mounts here in Phase 4.1 -->
    </section>
  {/if}
</main>

<nav class="bottom-nav" aria-label="Mobile navigation">
  <button type="button" data-bottom-route="#/" onclick={() => go(dashboardHash())}>Dashboard</button>
  <button type="button" data-bottom-action="new-task">New</button>
</nav>
