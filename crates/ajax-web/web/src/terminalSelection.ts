/**
 * Pure math for touch (long-press) text selection on the canvas terminal.
 *
 * ghostty-web renders to a <canvas>, so native browser selection can never
 * work; Ajax synthesizes it from touch gestures instead. These helpers map
 * touch points to terminal cells and order a dragged range so the gesture
 * wiring in terminalGestures and the terminal plumbing in TerminalRawView
 * stay thin and the math stays unit-testable.
 */

export interface CellPoint {
  col: number;
  row: number;
}

/**
 * Map a point (px, relative to the rendered grid's top-left) to the cell it
 * falls in, clamped into the grid so a drag past any edge selects to that
 * edge instead of vanishing. Returns undefined when the grid has no
 * measurable size (pre-layout, jsdom).
 */
export function cellAtPoint(
  xPx: number,
  yPx: number,
  gridWidthPx: number,
  gridHeightPx: number,
  cols: number,
  rows: number,
): CellPoint | undefined {
  if (
    !Number.isFinite(xPx) ||
    !Number.isFinite(yPx) ||
    !Number.isFinite(gridWidthPx) ||
    !Number.isFinite(gridHeightPx) ||
    gridWidthPx <= 0 ||
    gridHeightPx <= 0 ||
    !Number.isInteger(cols) ||
    !Number.isInteger(rows) ||
    cols <= 0 ||
    rows <= 0
  ) {
    return undefined;
  }
  const col = Math.min(cols - 1, Math.max(0, Math.floor((xPx / gridWidthPx) * cols)));
  const row = Math.min(rows - 1, Math.max(0, Math.floor((yPx / gridHeightPx) * rows)));
  return { col, row };
}

/**
 * Order two selection endpoints into reading order (top-left first), so a
 * drag upward/backward selects the same range as the forward drag.
 */
export function orderedSelection(
  a: CellPoint,
  b: CellPoint,
): { start: CellPoint; end: CellPoint } {
  const backward = a.row > b.row || (a.row === b.row && a.col > b.col);
  return backward ? { start: b, end: a } : { start: a, end: b };
}
