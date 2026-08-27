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

/** The one operation in flight when no tool row is visible yet. Once tools
 * appear, TurnActivity switches the summary to the counted line instead. */
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
