import type { WebAction } from "./types";

// Hide `resume`/`open` from the action UI: opening a task in the cockpit already
// dispatches the resume operation (see App.svelte `resumeOnOpen`), so a button
// would just duplicate the implicit view=resume gesture.
export function visibleTaskActions(actions: WebAction[]): WebAction[] {
  return actions.filter((action) => action.action !== "resume" && action.action !== "open");
}
