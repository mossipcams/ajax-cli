export { OPEN_FAILURE } from "./transport/contracts";

/** Map opaque ACP error strings to operator-facing copy. Human messages pass through. */
export function explainAcpError(message: string): string {
  if (/internal error/i.test(message)) {
    return "The agent rejected that request. Try sending again, or reopen the session.";
  }
  if (/ACP process exited/i.test(message)) {
    return "The agent stopped. It will restart when you reconnect.";
  }
  if (/acp request timed out/i.test(message)) {
    return "The agent did not answer in time. Try sending again.";
  }
  if (
    /cannot block the current thread|would block|block_in_place|within a runtime/i.test(
      message,
    )
  ) {
    return "Could not save the selected model. Try again in a moment.";
  }
  if (/session task stopped|session task dropped reply/i.test(message)) {
    return "The session worker stopped. Reopen the session to try again.";
  }
  return message;
}

/** `prepare_task_session` refuses the upgrade when the task cannot host an
 * orchestration session or its worktree is gone. Both facts are already in the
 * detail payload, so no extra request is needed to say which one it was. */
export function explainOpenFailure(
  detail: {
    agent?: string | null;
    status_explanation?: string | null;
    session_capable?: boolean;
  } | null,
): string {
  if (detail?.session_capable === false) {
    const agent = detail.agent?.trim();
    if (agent) {
      return `This task cannot host orchestration chat while ${agent} is running in the terminal. Open the task view instead.`;
    }
    return "This task cannot host orchestration chat. Open the task view instead.";
  }
  const explanation = detail?.status_explanation?.trim();
  if (explanation) {
    return `Can't start the session: ${explanation}`;
  }
  return "Can't start the session. Check the task still exists and its worktree is present.";
}
