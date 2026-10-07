export const TOOL_TONES: Record<string, string> = {
  read: "muted",
  edit: "running",
  delete: "error",
  move: "running",
  search: "muted",
  execute: "running",
  think: "muted",
  fetch: "muted",
};

export const TOOL_MARKS: Record<string, string> = {
  read: "◦",
  edit: "±",
  delete: "×",
  move: "→",
  search: "⌕",
  execute: "$",
  think: "∴",
  fetch: "↓",
  switch_mode: "⇄",
};

export function toolMark(kind: string): string {
  return TOOL_MARKS[kind] ?? "•";
}

export const TOOL_STATUS_LABELS: Record<string, string> = {
  pending: "queued",
  in_progress: "running",
  completed: "done",
  failed: "failed",
  cancelled: "stopped",
};

export function toolStatusLabel(status: string): string {
  return TOOL_STATUS_LABELS[status] ?? status;
}

export function toolStatusNote(status: string): string | null {
  return status === "completed" ? null : (TOOL_STATUS_LABELS[status] ?? status);
}

export function cleanTitle(title: string): string {
  return title.replace(/`/g, "").trim();
}

const GENERIC_TOOL_TITLES = new Set([
  "read file",
  "edit file",
  "delete file",
  "find",
  "grep",
  "search files",
  "mcp: tool",
  "mcp tool",
]);

export function isGenericToolTitle(title: string): boolean {
  const normalized = cleanTitle(title).toLowerCase();
  if (GENERIC_TOOL_TITLES.has(normalized)) return true;
  return normalized === "tool" || normalized === "mcp";
}

export function mcpToolNameFromTitle(title: string): string | null {
  const match = cleanTitle(title).match(/^mcp:\s*(.+)$/i);
  if (!match) return null;
  const name = match[1]?.trim() ?? "";
  if (!name || name.toLowerCase() === "tool") return null;
  return name;
}

export function shortCommand(command: string): string {
  const firstLine = command.trim().split("\n")[0]?.trim() ?? command.trim();
  const firstClause = firstLine.split(/\s&&\s|\s;\s|\s\|\s/)[0]?.trim() ?? firstLine;
  const max = 96;
  if (firstClause.length <= max) return firstClause;
  return `${firstClause.slice(0, max - 1)}…`;
}

export function toolTarget(call: {
  kind?: string;
  title: string;
  locations: string[];
  callId: string;
}): string {
  const location = call.locations[0];
  if (location) {
    if (call.kind === "execute") return shortCommand(location);
    return shortPath(location);
  }
  const title = cleanTitle(call.title);
  if (isGenericToolTitle(title)) return mcpToolNameFromTitle(title) ?? "";
  if (call.kind === "execute") return shortCommand(title);
  return mcpToolNameFromTitle(title) ?? (title || call.callId);
}

export const OPERATION_VERBS: Record<string, string> = {
  read: "Reading",
  edit: "Editing",
  delete: "Deleting",
  move: "Moving",
  search: "Searching",
  execute: "Running",
  think: "Thinking about",
  fetch: "Fetching",
};

export const OPERATION_VERBS_PAST: Record<string, string> = {
  read: "Read",
  edit: "Edited",
  delete: "Deleted",
  move: "Moved",
  search: "Searched",
  execute: "Ran",
  think: "Thought about",
  fetch: "Fetched",
};

export function toolRowTarget(call: {
  kind: string;
  title: string;
  locations: string[];
  callId: string;
}): string {
  const location = call.locations[0];
  if (location) {
    if (call.kind === "execute") return shortCommand(location);
    if (call.kind === "search") return location;
    const parts = location.split("/").filter(Boolean);
    return parts[parts.length - 1] ?? shortPath(location);
  }
  const title = cleanTitle(call.title);
  if (isGenericToolTitle(title)) return mcpToolNameFromTitle(title) ?? "";
  if (call.kind === "execute") return shortCommand(title);
  const mcpName = mcpToolNameFromTitle(title);
  if (mcpName) return mcpName;
  if (title) return title;
  return "";
}

export function toolRowLabel(call: {
  kind: string;
  title: string;
  locations: string[];
  callId: string;
}): string {
  const verb = OPERATION_VERBS_PAST[call.kind] ?? "Used";
  const target = toolRowTarget(call);
  if (!target || isGenericToolTitle(target)) return verb;
  return `${verb} ${target}`;
}

const TOKEN_BOUNDARY = /[\s/\\:;,.|]/;

function isTokenBoundary(text: string, index: number): boolean {
  if (index <= 0 || index >= text.length) return true;
  return TOKEN_BOUNDARY.test(text[index - 1]!) || TOKEN_BOUNDARY.test(text[index]!);
}

export function middleSplit(text: string, tail = 14): [string, string] {
  if (text.length <= tail * 2) return [text, ""];

  const targetTailStart = text.length - tail;
  let splitAt = targetTailStart;
  while (splitAt > 0 && !isTokenBoundary(text, splitAt)) splitAt -= 1;

  if (splitAt < text.length - tail * 2) {
    splitAt = targetTailStart;
    while (splitAt < text.length && !isTokenBoundary(text, splitAt)) splitAt += 1;
  }

  if (splitAt <= 0 || splitAt >= text.length) return [text, ""];
  return [text.slice(0, splitAt), text.slice(splitAt)];
}

export const CONTENT_PREVIEW_LINES = 8;

export function textPreview(
  text: string,
  maxLines: number,
  fromEnd: boolean,
): { preview: string; hiddenLines: number } {
  const lines = text.split("\n");
  if (lines.length <= maxLines) return { preview: text, hiddenLines: 0 };
  if (fromEnd) {
    return {
      preview: lines.slice(-maxLines).join("\n"),
      hiddenLines: lines.length - maxLines,
    };
  }
  return {
    preview: lines.slice(0, maxLines).join("\n"),
    hiddenLines: lines.length - maxLines,
  };
}

export function formatElapsed(ms: number | undefined): string | null {
  if (ms === undefined || ms < 1000) return null;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return seconds % 60 ? `${minutes}m ${seconds % 60}s` : `${minutes}m`;
  return minutes % 60 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${Math.floor(minutes / 60)}h`;
}

export function elapsedMs(call: { startedAt?: number; endedAt?: number }): number | undefined {
  if (call.startedAt === undefined || call.endedAt === undefined) return undefined;
  return call.endedAt - call.startedAt;
}

export function shortPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 2) return parts.join("/");
  return `…/${parts.slice(-2).join("/")}`;
}

export type DiffLine = { sign: " " | "-" | "+"; text: string };

export function diffLines(oldText: string, newText: string): DiffLine[] {
  const before = oldText.length ? oldText.split("\n") : [];
  const after = newText.length ? newText.split("\n") : [];

  let head = 0;
  while (head < before.length && head < after.length && before[head] === after[head]) head += 1;
  let tail = 0;
  while (
    tail < before.length - head &&
    tail < after.length - head &&
    before[before.length - 1 - tail] === after[after.length - 1 - tail]
  ) {
    tail += 1;
  }

  if (head === before.length && head === after.length) return [];

  const context = 2;
  const leading = Math.max(0, head - context);
  const lines: DiffLine[] = [];
  for (const text of before.slice(leading, head)) lines.push({ sign: " ", text });
  for (const text of before.slice(head, before.length - tail)) lines.push({ sign: "-", text });
  for (const text of after.slice(head, after.length - tail)) lines.push({ sign: "+", text });
  const trailingEnd = Math.min(before.length, before.length - tail + context);
  for (const text of before.slice(before.length - tail, trailingEnd)) {
    lines.push({ sign: " ", text });
  }
  return lines;
}
