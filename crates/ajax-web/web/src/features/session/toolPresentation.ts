// How an ACP tool call reads on the surface: its tone, its mark, its path, and
// the shape of the diff it wrote. Shared by the live head (one running call)
// and the conversation (every call, kept in place).

/** ACP `ToolKind` → the route's tone vocabulary. Kinds that change the worktree
 * carry the running tone; kinds that only look carry none. */
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

/** One glyph per kind. Mono marks, not an icon set: this column is the CLI
 * speaking, and a drawn icon would be the only illustration on the surface. */
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
};

export function toolStatusLabel(status: string): string {
  return TOOL_STATUS_LABELS[status] ?? status;
}

/** Paths are long and their tail is the informative end, so keep the last two
 * segments rather than ellipsizing the filename away. */
export function shortPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 2) return parts.join("/");
  return `…/${parts.slice(-2).join("/")}`;
}

export type DiffLine = { sign: " " | "-" | "+"; text: string };

/** Line diff for an ACP `ToolCallContent::Diff`, which carries whole file texts.
 * Printing both in full would bury the edit, so this trims the lines the two
 * sides share at each end and shows what is left.
 *
 * ponytail: single hunk — an edit touching two distant regions renders as one
 * span covering both. Swap in an LCS diff if multi-hunk edits become common
 * enough to read badly.
 */
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

  // Nothing differs: a diff with no change is no diff, and context lines with
  // no sign between them would read as an edit that did nothing.
  if (head === before.length && head === after.length) return [];

  // Two lines of shared text on each side: enough to place the change in the
  // file, not so much that the change stops being the thing you see.
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
