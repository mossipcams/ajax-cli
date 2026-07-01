<script lang="ts">
  import { onMount } from "svelte";
  import { fetchTaskSnapshot, sendTaskKeys } from "../api";

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  // Polling the pane snapshot is the default mobile experience: it survives Safari
  // backgrounding trivially (stateless HTTP), reads the visible pane so it works
  // even when the agent is in the alt-screen buffer, and never opens the fragile
  // raw attach socket. Instructions go through the composer (send-keys), so most
  // interactions never need raw keystroke streaming at all.
  const POLL_MS = 1500;

  let lines = $state<string[]>([]);
  let summary = $state<string | null>(null);
  let status = $state<"loading" | "live" | "error">("loading");
  let errorDetail = $state("");
  let composerText = $state("");
  let sending = $state(false);
  let sendError = $state("");

  let sequence: string | undefined;
  let viewport: HTMLDivElement | undefined = $state();
  let pinnedToBottom = true;

  const updatePinned = () => {
    if (!viewport) return;
    const slack = 24;
    pinnedToBottom =
      viewport.scrollTop + viewport.clientHeight >= viewport.scrollHeight - slack;
  };

  const scrollToBottomIfPinned = () => {
    queueMicrotask(() => {
      if (pinnedToBottom && viewport) viewport.scrollTop = viewport.scrollHeight;
    });
  };

  const refresh = async () => {
    try {
      const snapshot = await fetchTaskSnapshot(handle, sequence);
      sequence = snapshot.sequence;
      status = "live";
      errorDetail = "";
      // Only replace the rendered lines when the pane actually changed so an idle
      // pane doesn't reset the user's scroll position on every poll.
      if (snapshot.sequence_changed) {
        lines = snapshot.lines;
        summary = snapshot.summary;
        scrollToBottomIfPinned();
      }
    } catch (error) {
      status = "error";
      errorDetail = error instanceof Error ? error.message : String(error);
    }
  };

  const submit = async () => {
    const text = composerText.trim();
    if (!text || sending) return;
    sending = true;
    sendError = "";
    const result = await sendTaskKeys(handle, text, true);
    sending = false;
    if (result.ok) {
      composerText = "";
      // Reflect the effect quickly instead of waiting out the poll interval.
      refresh();
    } else {
      sendError = result.error ?? "send failed";
    }
  };

  const onComposerKeydown = (event: KeyboardEvent) => {
    // Enter submits; Shift+Enter is reserved for a literal newline.
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  };

  onMount(() => {
    refresh();
    const timer = setInterval(refresh, POLL_MS);
    return () => clearInterval(timer);
  });
</script>

<section class="terminal-snapshot" data-testid="task-terminal-snapshot" aria-label="Task terminal snapshot">
  <div class="terminal-snapshot-head">
    <span class="terminal-snapshot-status" data-testid="snapshot-status">
      {#if status === "error"}Disconnected{:else if status === "loading"}Loading…{:else}Live{/if}
    </span>
    {#if summary}
      <span class="terminal-snapshot-summary">{summary}</span>
    {/if}
  </div>

  <div
    class="terminal-snapshot-lines task-terminal-snapshot-viewport"
    bind:this={viewport}
    onscroll={updatePinned}
    role="log"
    aria-live="polite"
  >
    {#if status === "error"}
      <p class="terminal-snapshot-error">Could not read the terminal: {errorDetail}</p>
    {:else}
      <pre>{lines.join("\n")}</pre>
    {/if}
  </div>

  <form
    class="terminal-composer"
    onsubmit={(event) => {
      event.preventDefault();
      submit();
    }}
  >
    <input
      class="terminal-composer-input"
      type="text"
      inputmode="text"
      autocapitalize="off"
      autocorrect="off"
      autocomplete="off"
      spellcheck="false"
      placeholder="Type an instruction and press Enter…"
      bind:value={composerText}
      onkeydown={onComposerKeydown}
      aria-label="Terminal command"
    />
    <button type="submit" class="terminal-composer-send" disabled={sending || !composerText.trim()}>
      {sending ? "Sending…" : "Send"}
    </button>
  </form>
  {#if sendError}
    <div class="terminal-composer-error" data-testid="composer-error">{sendError}</div>
  {/if}
</section>

<style>
  .terminal-snapshot {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }

  .terminal-snapshot-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--rule);
  }

  .terminal-snapshot-status {
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .terminal-snapshot-summary {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--ink);
  }

  .terminal-snapshot-lines {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
    overscroll-behavior: contain;
    padding: 8px;
  }

  .terminal-snapshot-lines pre {
    margin: 0;
    font-family: var(--mono);
    font-size: 14px;
    line-height: 1.35;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--ink);
  }

  .terminal-snapshot-error {
    margin: 0;
    font-size: 12px;
    color: var(--ink-muted);
  }

  .terminal-composer {
    display: flex;
    gap: 8px;
    padding: 8px;
    border-top: 1px solid var(--rule);
  }

  .terminal-composer-input {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 40px;
    padding: 8px 10px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: var(--paper);
    color: var(--ink);
    font-family: var(--mono);
    font-size: 14px;
  }

  .terminal-composer-send {
    flex: none;
    min-width: 64px;
    min-height: 40px;
    padding: 8px 14px;
    border: 1px solid var(--teal);
    border-radius: var(--radius-sm);
    background: var(--teal-deep);
    color: var(--paper);
    font-size: 13px;
    font-weight: 700;
  }

  .terminal-composer-send:disabled {
    opacity: 0.5;
  }

  .terminal-composer-error {
    padding: 0 8px 8px;
    font-size: 12px;
    color: var(--danger, #b3402f);
  }
</style>
