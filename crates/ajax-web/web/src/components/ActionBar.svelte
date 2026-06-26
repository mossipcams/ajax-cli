<script lang="ts">
  import type { BrowserCockpitView, WebAction } from "../types";
  import { actionLabel } from "../state";
  import { CONFIRM_TIMEOUT_MS } from "../polling";
  import { postOperation, requestId } from "../api";

  interface Props {
    actions: WebAction[];
    handle: string;
    /** Refreshed cockpit projection returned by a mutation. */
    onCockpit?: (cockpit: BrowserCockpitView) => void;
    /** Surface the operation result for the result banner. */
    onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
    /** Notify the parent a mutation finished (e.g. to refresh detail). */
    onMutated?: () => void;
  }

  let { actions, handle, onCockpit, onResult, onMutated }: Props = $props();

  let pendingAction = $state<string | null>(null);
  let runningAction = $state<string | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => () => {
    if (confirmTimer) clearTimeout(confirmTimer);
  });

  const REMEDIATION = new Set(["fix-ci", "resolve-merge-conflicts"]);

  function clearConfirm() {
    if (confirmTimer) clearTimeout(confirmTimer);
    confirmTimer = null;
    pendingAction = null;
  }

  function label(action: WebAction): string {
    if (pendingAction === action.action) return "Tap to confirm";
    if (runningAction === action.action) return `${actionLabel(action)} …`;
    return actionLabel(action);
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
        onResult?.(`${actionLabel(action)} completed`, result.response.output, false);
        onMutated?.();
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
