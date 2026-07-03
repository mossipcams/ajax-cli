import type { WebAction } from "./types";

export function visibleTaskActions(actions: WebAction[]): WebAction[] {
  return actions.filter((action) => action.action !== "resume" && action.action !== "open");
}
