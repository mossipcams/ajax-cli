import type { ConversationItem, ToolCall } from "../session/public";
import { elapsedMs, formatElapsed } from "./presentation";

function tools(items: ConversationItem[]): ToolCall[] {
  return items.flatMap((item) => (item.kind === "tool" ? [item.call] : []));
}

function count(n: number, singular: string, plural: string): string {
  return `${n} ${n === 1 ? singular : plural}`;
}

const SUMMARY_KINDS = new Set(["read", "edit", "move", "delete", "search", "execute"]);

export function activitySummary(items: ConversationItem[]): string {
  const calls = tools(items);
  const kinds = (...wanted: string[]) =>
    calls.filter((call) => wanted.includes(call.kind)).length;

  const parts: string[] = [];
  const read = kinds("read");
  const edited = kinds("edit", "move", "delete");
  const searched = kinds("search");
  const ran = kinds("execute");
  const other = calls.filter((call) => !SUMMARY_KINDS.has(call.kind)).length;
  if (read) parts.push(`read ${count(read, "file", "files")}`);
  if (edited) parts.push(`edited ${count(edited, "file", "files")}`);
  if (searched) parts.push(`searched ${count(searched, "query", "queries")}`);
  if (ran) parts.push(`ran ${count(ran, "command", "commands")}`);
  if (other) parts.push(`used ${count(other, "tool", "tools")}`);

  if (!parts.length) {
    if (items.some((item) => item.kind === "plan")) parts.push("planning");
    else if (items.some((item) => item.kind === "thought")) parts.push("reasoning");
    else parts.push(count(items.length, "step", "steps"));
  }

  const failed = calls.filter((call) => call.status === "failed").length;
  if (failed) parts.push(`${failed} failed`);

  const first = calls.find((call) => call.startedAt !== undefined);
  const last = [...calls].reverse().find((call) => call.endedAt !== undefined);
  const span = formatElapsed(
    first && last ? elapsedMs({ startedAt: first.startedAt, endedAt: last.endedAt }) : undefined,
  );
  if (span) parts.push(span);

  const line = parts.join(" · ");
  return line.charAt(0).toUpperCase() + line.slice(1);
}
