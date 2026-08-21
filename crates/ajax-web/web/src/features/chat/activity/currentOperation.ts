import {
  activePlanStep,
  thoughtSnippet,
  type ConversationItem,
  type ToolCall,
} from "../session/public";
import { OPERATION_VERBS, toolTarget } from "./presentation";

function tools(items: ConversationItem[]): ToolCall[] {
  return items.flatMap((item) => (item.kind === "tool" ? [item.call] : []));
}

/** The one operation in flight. The card replaces this line rather than growing
 * a row per call: on a phone the log is the disclosure's job, not the turn's. */
export function currentOperation(items: ConversationItem[]): string {
  const running = tools(items)
    .filter((call) => call.status === "pending" || call.status === "in_progress")
    .pop();
  if (running) {
    return `${OPERATION_VERBS[running.kind] ?? "Working on"} ${toolTarget(running)}…`;
  }
  for (let i = items.length - 1; i >= 0; i -= 1) {
    const item = items[i];
    if (item.kind === "plan") {
      const step = activePlanStep(item.entries);
      if (step) return `${step}…`;
    }
    if (item.kind === "thought") return `${thoughtSnippet(item.text, 60)}`;
  }
  return "Working…";
}
