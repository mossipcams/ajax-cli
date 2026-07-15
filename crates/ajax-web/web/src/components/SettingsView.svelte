<script lang="ts">
  import { restartServer, waitForServerOnline } from "../api";
  import { buildDiagnosticsReport, copyText } from "../diagnostics";
  import { CONFIRM_TIMEOUT_MS } from "../polling";
  import {
    isTerminalSurfaceV2Enabled,
    setTerminalSurfaceV2Enabled,
    subscribeTerminalSurfaceV2,
  } from "../terminalSurfaceSetting";

  interface Props {
    detailHandle?: string | null;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
    onRestarted?: () => void;
    onBack?: () => void;
  }

  let { detailHandle = null, onResult, onRestarted, onBack }: Props = $props();

  let confirmingRestart = $state(false);
  let restartStatus = $state<string | null>(null);
  let restarting = $state(false);
  let diagnosticsOutput = $state<string | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | null = null;
  let terminalSurfaceV2 = $state(isTerminalSurfaceV2Enabled());

  $effect(() => {
    const unsubscribe = subscribeTerminalSurfaceV2((enabled) => {
      terminalSurfaceV2 = enabled;
    });
    return unsubscribe;
  });

  async function restart() {
    if (!confirmingRestart) {
      confirmingRestart = true;
      confirmTimer = setTimeout(() => (confirmingRestart = false), CONFIRM_TIMEOUT_MS);
      return;
    }
    if (confirmTimer) clearTimeout(confirmTimer);
    confirmingRestart = false;
    restarting = true;
    restartStatus = "Restarting…";
    try {
      await restartServer();
    } catch {
      // A connection drop during restart is expected.
    }
    const online = await waitForServerOnline();
    restarting = false;
    if (online) {
      restartStatus = null;
      onResult?.("Server restarted", null, false);
      onRestarted?.();
    } else {
      restartStatus = null;
      onResult?.("Server did not come back in time", null, true);
    }
  }

  async function runDiagnostics() {
    diagnosticsOutput = "Running diagnostics...";
    const report = await buildDiagnosticsReport(detailHandle);
    diagnosticsOutput = JSON.stringify(report, null, 2);
  }

  async function copyDiagnostics() {
    const report = await buildDiagnosticsReport(detailHandle);
    const text = JSON.stringify(report, null, 2);
    diagnosticsOutput = text;
    const copied = await copyText(text);
    onResult?.(copied ? "Diagnostics copied" : "Diagnostics ready to copy", null, false);
  }

  function reloadApp() {
    window.location.reload();
  }

  const appVersion =
    document.querySelector<HTMLMetaElement>('meta[name="ajax-app-version"]')?.content ?? "—";
  const origin = window.location.origin;
  const online = navigator.onLine;
  const surfaceV2LastError = sessionStorage.getItem("ajax.terminal.surfaceV2.lastError");
  const truncatedUa =
    navigator.userAgent.length > 80
      ? `${navigator.userAgent.slice(0, 80)}…`
      : navigator.userAgent;
</script>

<section class="settings-view" aria-labelledby="settings-heading">
  <div class="settings-header">
    <button type="button" class="settings-back pill" onclick={() => onBack?.()}>Back</button>
    <h2 id="settings-heading">Settings</h2>
  </div>

  <div class="settings-section" data-testid="dev-settings">
    <h3>Dev settings</h3>

    <h4 class="settings-subheading">Experiments</h4>
    <label class="settings-toggle">
      <input
        type="checkbox"
        data-testid="setting-terminal-surface-v2"
        checked={terminalSurfaceV2}
        onchange={(event) => {
          const enabled = (event.currentTarget as HTMLInputElement).checked;
          setTerminalSurfaceV2Enabled(enabled);
          terminalSurfaceV2 = enabled;
        }} />
      Terminal Surface V2
    </label>
    <p class="settings-note">
      Experimental xterm.js terminal for mobile bake-off.
    </p>

    <h4 class="settings-subheading">Debug info</h4>
    <dl class="settings-debug" data-testid="dev-settings-debug">
      <div><dt>App version</dt><dd>{appVersion}</dd></div>
      <div><dt>Origin</dt><dd>{origin}</dd></div>
      <div><dt>Online</dt><dd>{online ? "yes" : "no"}</dd></div>
      <div><dt>Surface V2</dt><dd>{terminalSurfaceV2 ? "on" : "off"}</dd></div>
      <div><dt>Last error</dt><dd>{surfaceV2LastError ?? "—"}</dd></div>
      <div><dt>User agent</dt><dd>{truncatedUa}</dd></div>
    </dl>

    <h4 class="settings-subheading">Actions</h4>
    <button type="button" class="pill" onclick={reloadApp}>Reload app</button>
    <button type="button" class="pill" onclick={runDiagnostics}>Run diagnostics</button>
    <button type="button" class="pill" onclick={copyDiagnostics}>Copy Diagnostics</button>
    <p class="settings-note">Restarts this Cockpit process.</p>
    <button type="button" class="pill" disabled={restarting} onclick={restart}>
      {confirmingRestart ? "Tap to confirm" : "Restart server"}
    </button>
    {#if restartStatus}
      <p class="settings-status">{restartStatus}</p>
    {/if}
    {#if diagnosticsOutput}
      <pre class="settings-status">{diagnosticsOutput}</pre>
    {/if}
  </div>
</section>

<style>
  /* SETTINGS VIEW — rendered only on the settings route (legacy used a
     body.view-settings toggle; routing replaces it). */
  .settings-view {
    display: block;
  }

  .settings-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 18px;
  }

  .settings-header h2 {
    margin: 0;
    font-size: 21px;
    font-weight: 700;
    letter-spacing: 0.01em;
    line-height: 1.25;
    text-transform: none;
    color: var(--ink);
    flex: 1 1 auto;
    overflow-wrap: anywhere;
  }

  .settings-back {
    flex: none;
    min-height: 44px;
    padding: 7px 16px;
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }

  .settings-section {
    margin-top: 22px;
    padding-top: 16px;
    border-top: 1px solid var(--rule);
  }

  .settings-section h3 {
    margin: 0 0 10px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .settings-subheading {
    margin: 16px 0 8px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
    color: var(--ink-soft);
  }

  .settings-subheading:first-of-type {
    margin-top: 0;
  }

  .settings-debug {
    margin: 0 0 14px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--ink-soft);
  }

  .settings-debug div {
    display: flex;
    gap: 8px;
    margin-bottom: 4px;
  }

  .settings-debug dt {
    flex: none;
    min-width: 88px;
    font-weight: 600;
    color: var(--ink-muted);
  }

  .settings-debug dd {
    margin: 0;
    font-family: var(--mono);
    overflow-wrap: anywhere;
  }

  .settings-section .pill {
    margin-right: 8px;
    margin-bottom: 8px;
  }

  .settings-note {
    margin: 0 0 14px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--ink-soft);
  }

  .settings-note :global(code) {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--mustard-bright);
  }

  .settings-status {
    margin: 12px 0 0;
    font-size: 13px;
    color: var(--ink-muted);
    font-family: var(--mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .settings-toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
    color: var(--ink);
    cursor: pointer;
    user-select: none;
  }

  .settings-toggle input {
    width: 18px;
    height: 18px;
    accent-color: var(--mustard-bright);
  }
</style>
