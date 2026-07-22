export type ZeroLagCursorMetrics = {
  cursorX: number;
  cursorY: number;
  cols: number;
  rows: number;
  canvasWidth: number;
  canvasHeight: number;
  cellWidth?: number;
  cellHeight?: number;
  fontSize: number;
};

export const ZERO_LAG_OVERLAY_CLASS = "xterm-zerolag-input";
export const ZERO_LAG_OVERLAY_TESTID = "xterm-zerolag-input";

export type ZeroLagMeasureInput = {
  cursorX?: number;
  cursorY?: number;
  cols: number;
  rows: number;
  canvasWidth: number;
  canvasHeight: number;
  cellWidth?: number;
  cellHeight?: number;
  fontSize: number;
};

export function measureZeroLagCursor(
  input: ZeroLagMeasureInput | null | undefined,
): ZeroLagCursorMetrics | null {
  if (input == null) return null;
  if (input.cursorX === undefined || input.cursorY === undefined) return null;
  return {
    cursorX: input.cursorX,
    cursorY: input.cursorY,
    cols: input.cols,
    rows: input.rows,
    canvasWidth: input.canvasWidth,
    canvasHeight: input.canvasHeight,
    cellWidth: input.cellWidth,
    cellHeight: input.cellHeight,
    fontSize: input.fontSize,
  };
}

export function measureZeroLagFromXtermHost(args: {
  host: HTMLElement | null | undefined;
  term: {
    cols: number;
    rows: number;
    options: { fontSize?: number };
    buffer: { active: { cursorX?: number; cursorY?: number } };
  } | null | undefined;
  defaultFontSize: number;
}): ZeroLagCursorMetrics | null {
  const { host, term, defaultFontSize } = args;
  const screen = host?.querySelector<HTMLElement>(".xterm-screen");
  const active = term?.buffer.active;
  if (!screen || !term || active?.cursorX === undefined || active.cursorY === undefined) {
    return null;
  }
  const row = host?.querySelector(".xterm-rows > *");
  const cellHeight = row ? row.getBoundingClientRect().height : undefined;
  const cellWidth =
    cellHeight && cellHeight > 0 && term.cols > 0 ? screen.clientWidth / term.cols : undefined;
  return measureZeroLagCursor({
    cursorX: active.cursorX,
    cursorY: active.cursorY,
    cols: term.cols,
    rows: term.rows,
    canvasWidth: screen.clientWidth,
    canvasHeight: screen.clientHeight,
    cellWidth,
    cellHeight,
    fontSize: term.options.fontSize ?? defaultFontSize,
  });
}

export function createZeroLagOverlayPainter(
  host: HTMLElement | null | undefined | (() => HTMLElement | null | undefined),
): {
  paint(text: string, style: string): void;
  dispose(): void;
} {
  let el: HTMLDivElement | null = null;
  const getHost = typeof host === "function" ? host : () => host;

  const paint = (text: string, style: string) => {
    const container = getHost();
    if (!container) return;
    if (!text) {
      el?.remove();
      el = null;
      return;
    }
    if (!el) {
      el = document.createElement("div");
      el.className = ZERO_LAG_OVERLAY_CLASS;
      el.setAttribute("data-testid", ZERO_LAG_OVERLAY_TESTID);
      el.setAttribute("aria-hidden", "true");
      container.insertBefore(el, container.firstChild);
    }
    el.textContent = text;
    el.style.cssText = style;
  };

  const dispose = () => {
    el?.remove();
    el = null;
  };

  return { paint, dispose };
}

export function zeroLagOverlayStyle(m: ZeroLagCursorMetrics): string {
  const cellWidth =
    Number.isFinite(m.cellWidth) && (m.cellWidth as number) > 0
      ? (m.cellWidth as number)
      : m.canvasWidth / m.cols;
  const cellHeight =
    Number.isFinite(m.cellHeight) && (m.cellHeight as number) > 0
      ? (m.cellHeight as number)
      : m.canvasHeight / m.rows;
  if (
    !Number.isFinite(cellWidth) ||
    !Number.isFinite(cellHeight) ||
    cellWidth <= 0 ||
    cellHeight <= 0
  ) {
    return "";
  }
  const left = Math.max(0, m.cursorX) * cellWidth;
  const top = Math.max(0, m.cursorY) * cellHeight;
  return `left: ${left}px; top: ${top}px; font-size: ${m.fontSize}px; line-height: ${cellHeight}px;`;
}

/** Idle window after the last keystroke before an unmatched prediction is
 *  force-cleared, so a ghost can never persist as duplicated text. */
export const ZERO_LAG_IDLE_CLEAR_MS = 300;

export function createZeroLagEcho(options: {
  onChange: (text: string, style: string) => void;
  measure: () => ZeroLagCursorMetrics | null;
  /** Idle-clear delay; injectable for tests. Defaults to 300ms. */
  idleClearMs?: number;
  schedule?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  clearSchedule?: (id: ReturnType<typeof setTimeout>) => void;
}): {
  text(): string;
  noteBeforeInputPrintable(data: string): void;
  noteBeforeInputBackspace(): void;
  onTerminalData(data: string): void;
  clear(): void;
  clearIfEchoedIn(outputChunk: string): void;
  reset(): void;
} {
  let printableAhead = "";
  let backspacesAhead = 0;
  let pending = "";
  let anchor: { x: number; y: number } | null = null;

  const idleClearMs = options.idleClearMs ?? ZERO_LAG_IDLE_CLEAR_MS;
  const schedule = options.schedule ?? ((fn, ms) => setTimeout(fn, ms));
  const clearSchedule = options.clearSchedule ?? ((id) => clearTimeout(id));
  let idleTimer: ReturnType<typeof setTimeout> | undefined;

  const cancelIdleClear = () => {
    if (idleTimer !== undefined) {
      clearSchedule(idleTimer);
      idleTimer = undefined;
    }
  };

  const armIdleClear = () => {
    cancelIdleClear();
    if (pending) idleTimer = schedule(() => clearAll(), idleClearMs);
  };

  const notify = () => {
    if (!pending) {
      options.onChange("", "");
      return;
    }
    const measured = options.measure();
    const style = measured ? zeroLagOverlayStyle(measured) : "";
    options.onChange(pending, style);
  };

  const setPending = (next: string) => {
    const wasEmpty = pending === "";
    pending = next;
    if (!pending) {
      anchor = null;
    } else if (wasEmpty) {
      const measured = options.measure();
      anchor = measured ? { x: measured.cursorX, y: measured.cursorY } : null;
    }
    notify();
    armIdleClear();
  };

  const trimPending = () => {
    setPending(pending.slice(0, -1));
  };

  const clearAll = () => {
    printableAhead = "";
    backspacesAhead = 0;
    setPending("");
  };

  return {
    text() {
      return pending;
    },

    noteBeforeInputPrintable(data: string) {
      if (!data) return;
      printableAhead += data;
      setPending(pending + data);
    },

    noteBeforeInputBackspace() {
      backspacesAhead += 1;
      trimPending();
    },

    onTerminalData(data: string) {
      if (data === "\r") {
        clearAll();
        return;
      }

      if (data === "\x7f") {
        if (backspacesAhead > 0) {
          backspacesAhead -= 1;
        } else {
          trimPending();
        }
        return;
      }

      if (data.length === 1 && data.charCodeAt(0) >= 32) {
        if (printableAhead.startsWith(data)) {
          printableAhead = printableAhead.slice(data.length);
        } else {
          setPending(pending + data);
        }
      }
    },

    clear() {
      clearAll();
    },

    clearIfEchoedIn(outputChunk: string) {
      if (!pending) return;
      const measured = options.measure();
      if (measured && anchor && (measured.cursorX !== anchor.x || measured.cursorY !== anchor.y)) {
        clearAll();
        return;
      }
      if (outputChunk.includes(pending)) {
        clearAll();
        return;
      }
      if (outputChunk.length > 0 && pending.startsWith(outputChunk)) {
        for (let i = 0; i < outputChunk.length; i++) {
          if (outputChunk.charCodeAt(i) < 32) return;
        }
        setPending(pending.slice(outputChunk.length));
      }
    },

    reset() {
      clearAll();
    },
  };
}
