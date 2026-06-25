<script lang="ts">
  import { untrack } from "svelte";
  import type { BrowserPaneState, BrowserTaskDetail } from "../types";
  import { ApiError, fetchPane, postAnswer } from "../api";
  import { applyPaneDelta, type PaneBuffer } from "../state";
  import { MAX_LOG_ENTRIES, paneInterval } from "../polling";
  import { copyText } from "../diagnostics";

  interface Props {
    handle: string;
    detail: BrowserTaskDetail;
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  }

  let { handle, detail, onResult }: Props = $props();

  let buffer = $state<PaneBuffer>({ sequence: 0, lines: [] });
  let paneState = $state<BrowserPaneState | null>(null);
  let tmuxExists = $state<boolean>(true);
  let terminalOpen = $state<boolean>(false);

  let tmuxMissing = $derived(tmuxExists === false);
  let kind = $derived(paneState?.kind ?? detail.live_status_kind ?? null);
  let canAnswer = $derived(Boolean(paneState?.answerable && paneState?.fingerprint));
  let lines = $derived(buffer.lines.length ? buffer.lines : []);

  // Poll the pane on a state-aware cadence, resetting the buffer whenever the
  // selected task changes so a stale buffer can't bleed across tasks.
  $effect(() => {
    void handle;
    buffer = { sequence: 0, lines: [] };
    paneState = null;
    tmuxExists = true;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    async function tick() {
      if (stopped) return;
      // Read current buffer/kind without subscribing the effect to them, so
      // writing those same signals below cannot retrigger this effect.
      const since = untrack(() => buffer.sequence);
      try {
        const result = await fetchPane(handle, since);
        if (result.kind === "missing") {
          tmuxExists = false;
        } else if (result.kind === "conflict") {
          tmuxExists = result.snapshot.tmux_exists;
          paneState = result.snapshot.state;
        } else {
          buffer = applyPaneDelta(untrack(() => buffer), result.snapshot, MAX_LOG_ENTRIES);
          paneState = result.snapshot.state;
          tmuxExists = result.snapshot.tmux_exists;
        }
      } catch {
        // Network hiccup — keep the last buffer and retry on the next tick.
      }
      if (!stopped) {
        const stateKind = untrack(() => kind) ?? undefined;
        timer = setTimeout(tick, paneInterval({ hidden: false, stateKind }));
      }
    }

    void tick();
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
    };
  });

  async function sendAnswer(answer: "approve" | "deny") {
    const fingerprint = paneState?.fingerprint;
    if (!fingerprint) {
      onResult?.("This approval is no longer current — refresh the task", null, true);
      return;
    }
    try {
      await postAnswer(handle, { answer, fingerprint, request_id: crypto.randomUUID() });
    } catch (error) {
      if (error instanceof ApiError) {
        if (error.kind === "conflict") {
          onResult?.("The agent moved on before this approval was sent", null, true);
        } else if (error.kind === "terminal") {
          onResult?.("This prompt needs the terminal instead of the dashboard", null, true);
        } else if (error.kind === "rate-limit") {
          onResult?.("Slow down — too many actions in a short window", null, true);
        } else {
          onResult?.("Could not send answer", null, true);
        }
        return;
      }
      onResult?.("Could not send answer — network error", null, true);
    }
  }

  async function copyAttach() {
    if (detail.tmux_session) await copyText(`tmux attach -t ${detail.tmux_session}`);
  }

  async function copyOutput() {
    await copyText(lines.join("\n"));
  }
</script>

<section class="interact-panel">
  {#if tmuxMissing}
    <section class="needs-block">
      <div class="interact-card-label">Needs from you</div>
      <p class="interact-card-body">Tmux session is missing. Sync the task to recover.</p>
    </section>
  {:else if kind === "WaitingForApproval"}
    <section class="needs-block">
      <div class="interact-card-label">Needs from you</div>
      <p class="interact-card-body">The agent is blocked on an approval decision.</p>
      {#if paneState?.command}<code class="interact-card-body">{paneState.command}</code>{/if}
      {#if canAnswer}
        <div class="interact-card-actions">
          <button type="button" class="pill is-primary" onclick={() => sendAnswer("approve")}>Approve</button>
          <button type="button" class="pill is-danger" onclick={() => sendAnswer("deny")}>Deny</button>
        </div>
      {:else}
        <p class="interact-hint">Open the terminal below for this approval.</p>
      {/if}
    </section>
  {:else if kind === "WaitingForInput"}
    <section class="needs-block">
      <div class="interact-card-label">Needs from you</div>
      {#if paneState?.prompt}<p class="interact-card-body">{paneState.prompt}</p>{/if}
      <p class="interact-hint">Open the terminal below for free-form replies.</p>
    </section>
  {/if}

  <section class="escape-hatch">
    <div class="interact-card-label">Open in terminal</div>
    {#if tmuxMissing || !detail.tmux_session}
      <p class="interact-card-body">Terminal session unavailable — sync the task to recover.</p>
    {:else}
      <p class="escape-hatch-hint">Continue this task in your SSH session — tap to copy the command.</p>
      <div class="escape-hatch-row">
        <button type="button" class="pill is-primary" onclick={copyAttach}>Open in tmux</button>
        <button type="button" class="pill" onclick={copyOutput}>Copy output</button>
      </div>
      <code class="escape-hatch-cmd">tmux attach -t {detail.tmux_session}</code>
    {/if}
  </section>

  <details class="terminal-details" bind:open={terminalOpen}>
    <summary>View terminal output</summary>
    {#if tmuxMissing}
      <p class="interact-hint">Terminal session is unavailable for this task.</p>
    {:else}
      <pre class="activity-excerpt">{lines.length
        ? lines.join("\n")
        : (detail.agent_activity ?? "No live pane snapshot available.")}</pre>
    {/if}
  </details>
</section>
