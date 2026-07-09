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

export function createZeroLagEcho(options: {
  onChange: (text: string, style: string) => void;
  measure: () => ZeroLagCursorMetrics | null;
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
    pending = next;
    notify();
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
          // Ahead already trimmed the overlay; absorb Ghostty's echo only.
          backspacesAhead -= 1;
        } else {
          trimPending();
        }
        return;
      }

      if (data.length === 1 && data.charCodeAt(0) >= 32) {
        if (printableAhead.startsWith(data)) {
          // Ahead already painted; absorb Ghostty's echo without a DOM write.
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
