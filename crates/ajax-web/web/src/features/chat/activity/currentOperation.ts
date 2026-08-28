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

/** The one operation in flight when the collapsed summary has no tool rows yet.
 * Once tools exist, TurnActivity switches the summary to the counted line and
 * shows only in-flight tool rows until expand. */
export function currentOperation(items: ConversationItem[]): string {
  const running = tools(items)
    .filter((call) => call.status === "pending" || call.status === "in_progress")
    .pop();
  if (running) {
    const target = toolTarget(running);
    const verb = OPERATION_VERBS[running.kind] ?? "Working on";
    return target ? `${verb} ${target}…` : `${verb}…`;
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
