<script lang="ts">
  import { restartServer, waitForServerOnline } from "../api";
  import { buildDiagnosticsReport, copyText } from "../diagnostics";
  import { CONFIRM_TIMEOUT_MS } from "../polling";

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
</script>

<section class="settings-view" aria-labelledby="settings-heading">
  <div class="settings-header">
    <button type="button" class="settings-back pill" onclick={() => onBack?.()}>Back</button>
    <h2 id="settings-heading">Settings</h2>
  </div>

  <div class="settings-section">
    <h3>Web server</h3>
    <p class="settings-note">Restarts this Cockpit process.</p>
    <button type="button" class="pill" disabled={restarting} onclick={restart}>
      {confirmingRestart ? "Tap to confirm" : "Restart server"}
    </button>
    {#if restartStatus}
      <p class="settings-status">{restartStatus}</p>
    {/if}
  </div>

  <div class="settings-section">
    <h3>Diagnostics</h3>
    <button type="button" class="pill" onclick={runDiagnostics}>Run diagnostics</button>
    <button type="button" class="pill" onclick={copyDiagnostics}>Copy Diagnostics</button>
    {#if diagnosticsOutput}
      <pre class="settings-status">{diagnosticsOutput}</pre>
    {/if}
  </div>
</section>
