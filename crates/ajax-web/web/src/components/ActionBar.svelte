<script lang="ts">
  // Frozen duplicate for TaskDetail until S6 deletes this file; keep bugfixes in sync with ActionBar.tsx.
  import type { BrowserCockpitView, WebAction } from "../types";
  import { CONFIRM_TIMEOUT_MS, DROP_UNDO_MS } from "../polling";
  import { postOperation, requestId } from "../api";

  interface Props {
    actions: WebAction[];
    handle: string;
    /** Refreshed cockpit projection returned by a mutation. */
    onCockpit?: (cockpit: BrowserCockpitView) => void;
    /** Surface the operation result for the result banner. */
    onResult?: (
      message: string,
      output: string | null | undefined,
      isError: boolean,
      options?: { onUndo?: () => void; onCommit?: () => void },
    ) => void;
    /** Notify the parent a mutation finished (e.g. to refresh detail). */
    onMutated?: () => void;
    /** The task no longer exists (e.g. after Drop) — leave the detail page. */
    onDismiss?: () => void;
  }

  let { actions, handle, onCockpit, onResult, onMutated, onDismiss }: Props = $props();

  let pendingAction = $state<string | null>(null);
  let runningAction = $state<string | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | null = null;
  // Delayed-Drop state: the API is not called until the undo window elapses.
  let dropTimer: ReturnType<typeof setTimeout> | null = null;
  let dropResolved = false;

  $effect(() => () => {
    if (confirmTimer) clearTimeout(confirmTimer);
    if (dropTimer) clearTimeout(dropTimer);
  });

  const REMEDIATION = new Set(["fix-ci", "resolve-merge-conflicts"]);

  function clearConfirm() {
    if (confirmTimer) clearTimeout(confirmTimer);
    confirmTimer = null;
    pendingAction = null;
  }

  function clearDropTimer() {
    if (dropTimer) clearTimeout(dropTimer);
    dropTimer = null;
  }

  function label(action: WebAction): string {
    if (pendingAction === action.action) return "Tap to confirm";
    if (runningAction === action.action) return `${action.label} …`;
    return action.label;
  }

  async function run(action: WebAction) {
    runningAction = action.action;
    try {
      const result = await postOperation({
        task_handle: handle,
        action: action.action,
        request_id: requestId(),
      });
      if (result.response.cockpit) onCockpit?.(result.response.cockpit);
      if (result.ok) {
        onResult?.(`${action.label} completed`, result.response.output, false);
        // Drop removes the task; refreshing this detail would 404. Leave instead.
        if (action.action === "drop") onDismiss?.();
        else onMutated?.();
      } else {
        onResult?.(
          result.error?.message ?? "Action failed",
          result.response.output,
          true,
        );
      }
    } catch (error) {
      onResult?.("Action failed — network error", null, true);
    } finally {
      runningAction = null;
    }
  }

  // Arm the delayed-Drop undo window. The toast's Undo cancels (no API); the
  // timer or the toast's auto-dismiss commits by running the real Drop.
  function armDrop(action: WebAction) {
    dropResolved = false;
    runningAction = "drop";
    const commit = () => {
      if (dropResolved) return;
      dropResolved = true;
      clearDropTimer();
      void run(action);
    };
    const undo = () => {
      if (dropResolved) return;
      dropResolved = true;
      clearDropTimer();
      runningAction = null;
    };
    dropTimer = setTimeout(commit, DROP_UNDO_MS);
    onResult?.(`Dropping ${handle}…`, null, false, { onUndo: undo, onCommit: commit });
  }

  function handleClick(action: WebAction) {
    if (runningAction) return;
    const needsConfirm = action.destructive || action.confirmation_required;
    if (needsConfirm && pendingAction !== action.action) {
      clearConfirm();
      pendingAction = action.action;
      confirmTimer = setTimeout(clearConfirm, CONFIRM_TIMEOUT_MS);
      return;
    }
    clearConfirm();
    // Only Drop is delayed for pre-commit undo; other actions run immediately.
    if (action.action === "drop") {
      armDrop(action);
      return;
    }
    void run(action);
  }
</script>

<div class="action-row">
  {#each actions as action, index (action.action)}
    <button
      type="button"
      class="action"
      class:primary={index === 0}
      class:confirming={pendingAction === action.action}
      class:is-running={runningAction === action.action}
      class:remediation-action={REMEDIATION.has(action.action)}
      data-action={action.action}
      data-task={handle}
      data-destructive={action.destructive ? "true" : undefined}
      disabled={runningAction !== null && runningAction !== action.action}
      onclick={() => handleClick(action)}
    >
      {label(action)}
    </button>
  {/each}
</div>

<style>
  /* Row of action buttons (buttons themselves use the global .action styles). */
  .action-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
</style>
