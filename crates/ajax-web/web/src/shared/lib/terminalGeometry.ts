export const MIN_TERMINAL_COLS = 80;
export const DEFAULT_FONT_SIZE = 13;
export const MIN_FONT_SIZE = 7;
export const MAX_FONT_SIZE = 20;
export const FONT_STORAGE_KEY = "ajax.terminal.fontSize";

export function parsePersistedFontSize(raw: string | null): number {
  if (!raw) return DEFAULT_FONT_SIZE;
  const size = Number(raw);
  if (!Number.isFinite(size) || size < MIN_FONT_SIZE || size > MAX_FONT_SIZE) {
    return DEFAULT_FONT_SIZE;
  }
  return size;
}

export interface TerminalGeometryInput {
  proposedCols: number;
  proposedRows: number;
  hostWidth: number;
  hostHeight: number;
  cellWidth: number;
  cellHeight: number;
  fontSize: number;
}

export interface TerminalGeometryResult {
  cols: number;
  rows: number;
  scale: number;
  logicalWidth: number;
  logicalHeight: number;
}

export function computeTerminalGeometry(
  input: TerminalGeometryInput,
): TerminalGeometryResult | null {
  const {
    proposedCols,
    proposedRows,
    hostWidth,
    hostHeight,
    cellWidth,
    cellHeight,
    fontSize,
  } = input;

  if (
    !Number.isFinite(proposedCols) ||
    !Number.isFinite(proposedRows) ||
    !Number.isInteger(proposedCols) ||
    !Number.isInteger(proposedRows) ||
    proposedCols <= 0 ||
    proposedRows <= 0
  ) {
    return null;
  }

  if (proposedCols >= MIN_TERMINAL_COLS) {
    return {
      cols: proposedCols,
      rows: proposedRows,
      scale: 1,
      logicalWidth: hostWidth,
      logicalHeight: hostHeight,
    };
  }

  if (cellWidth <= 0 || cellHeight <= 0 || hostWidth <= 0 || hostHeight <= 0) {
    return null;
  }

  const cols = MIN_TERMINAL_COLS + Math.max(0, MAX_FONT_SIZE - fontSize);
  const scale = Math.min(1, hostWidth / (cols * cellWidth));
  const rows = Math.max(1, Math.ceil(hostHeight / (cellHeight * scale)));

  return {
    cols,
    rows,
    scale,
    logicalWidth: hostWidth / scale,
    logicalHeight: hostHeight / scale,
  };
}
