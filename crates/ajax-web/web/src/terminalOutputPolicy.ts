/**
 * Pure output-follow and resize-size decisions for the mobile terminal.
 *
 * Keeps scrollback compensation, pinned/unseen follow effects, and resize
 * size validation unit-testable without Ghostty, WebSocket, or DOM.
 */

/** Coalesce window for batched terminal writes (~one frame). */
export const TERMINAL_WRITE_FLUSH_MS = 16;
/** Flush early when queued output reaches this many characters. */
export const TERMINAL_WRITE_MAX_CHARS = 32_000;

export type TerminalWriteBatcher = {
  push(text: string): void;
  flush(): void;
  dispose(): void;
  /** Test/inspection: queued character count. */
  pendingChars(): number;
};

/** Batch decoded output text before a single write/flush callback. */
export function createTerminalWriteBatcher(options: {
  flushMs?: number;
  maxChars?: number;
  now?: () => number;
  schedule?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  clearSchedule?: (id: ReturnType<typeof setTimeout>) => void;
  onFlush: (combined: string) => void;
}): TerminalWriteBatcher {
  const flushMs = options.flushMs ?? TERMINAL_WRITE_FLUSH_MS;
  const maxChars = options.maxChars ?? TERMINAL_WRITE_MAX_CHARS;
  const schedule = options.schedule ?? setTimeout;
  const clearSchedule = options.clearSchedule ?? clearTimeout;
  const chunks: string[] = [];
  let pending = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  const cancelTimer = () => {
    if (timer !== undefined) {
      clearSchedule(timer);
      timer = undefined;
    }
  };

  const flush = () => {
    cancelTimer();
    if (chunks.length === 0) return;
    const combined = chunks.join("");
    chunks.length = 0;
    pending = 0;
    options.onFlush(combined);
  };

  return {
    push(text: string) {
      if (disposed || text.length === 0) return;
      const wasEmpty = chunks.length === 0;
      chunks.push(text);
      pending += text.length;
      if (wasEmpty) {
        timer = schedule(flush, flushMs);
      }
      if (pending >= maxChars) {
        flush();
      }
    },
    flush,
    dispose() {
      disposed = true;
      cancelTimer();
      chunks.length = 0;
      pending = 0;
    },
    pendingChars() {
      return pending;
    },
  };
}

/** Scroll delta to preserve reader position when scrollback grows. */
export function scrollbackGrowthCompensation(before: number, after: number): number {
  if (!Number.isFinite(before) || !Number.isFinite(after) || !(after > before)) {
    return 0;
  }
  return before - after;
}

/** Follow/unseen effects for a write given pinned-to-bottom state. */
export function outputFollowEffects(pinnedToBottom: boolean): {
  snapToBottom: boolean;
  markUnseenOutput: boolean;
} {
  if (pinnedToBottom) {
    return { snapToBottom: true, markUnseenOutput: false };
  }
  return { snapToBottom: false, markUnseenOutput: true };
}

/** Accept only finite positive integer cols/rows; otherwise fail closed. */
export function validTerminalSize(
  cols: number,
  rows: number,
): { cols: number; rows: number } | undefined {
  if (
    !Number.isFinite(cols) ||
    !Number.isFinite(rows) ||
    !Number.isInteger(cols) ||
    !Number.isInteger(rows) ||
    cols <= 0 ||
    rows <= 0
  ) {
    return undefined;
  }
  return { cols, rows };
}

/** Skip PTY resize when cols/rows match the last sent size; reset on reconnect. */
export function createResizeDedupe(send: (cols: number, rows: number) => void): {
  sendIfChanged(cols: number, rows: number): void;
  reset(): void;
} {
  let lastCols: number | undefined;
  let lastRows: number | undefined;

  return {
    sendIfChanged(cols: number, rows: number) {
      if (lastCols === cols && lastRows === rows) return;
      lastCols = cols;
      lastRows = rows;
      send(cols, rows);
    },
    reset() {
      lastCols = undefined;
      lastRows = undefined;
    },
  };
}
