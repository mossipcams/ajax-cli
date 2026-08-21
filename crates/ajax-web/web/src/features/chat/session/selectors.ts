import type {
  ChatSessionView,
  ConversationItem,
  PlanEntry,
  ToolCall,
} from "./model";

export function activeTool(view: ChatSessionView): ToolCall | null {
  let last: ToolCall | null = null;
  for (const item of view.conversation) {
    if (item.kind !== "tool") continue;
    const call = item.call;
    if (call.status === "pending" || call.status === "in_progress") last = call;
    else if (last === null || last.status === "completed" || last.status === "failed") {
      last = call;
    }
  }
  return last;
}

export function toolCount(items: ConversationItem[]): number {
  return items.reduce((n, item) => (item.kind === "tool" ? n + 1 : n), 0);
}

export function latestPlan(items: ConversationItem[]): PlanEntry[] {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    const item = items[i];
    if (item.kind === "plan") return item.entries;
  }
  return [];
}

export function activePlanStep(plan: PlanEntry[]): string | null {
  return plan.find((entry) => entry.status === "in_progress")?.content ?? null;
}

export function latestThought(items: ConversationItem[]): string | null {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    const item = items[i];
    if (item.kind === "thought") {
      const text = item.text.trim();
      return text || null;
    }
  }
  return null;
}

export function thoughtSnippet(text: string, maxLen = 120): string {
  const line = text.replace(/\s+/g, " ").trim();
  if (line.length <= maxLen) return line;
  const cut = line.lastIndexOf(" ", maxLen - 1);
  const end = cut > 0 ? cut : maxLen - 1;
  return `${line.slice(0, end)}…`;
}
