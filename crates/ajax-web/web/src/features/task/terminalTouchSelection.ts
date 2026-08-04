import type { Terminal } from "@xterm/xterm";

export function isWordChar(ch: string): boolean {
  if (!ch) return false;
  const code = ch.charCodeAt(0);
  return (
    (code >= 48 && code <= 57) ||
    (code >= 65 && code <= 90) ||
    (code >= 97 && code <= 122) ||
    code === 45 ||
    code === 95 ||
    code > 127
  );
}

export type BufferCell = { col: number; row: number };

export function screenBounds(
  term: Terminal,
  hostEl: HTMLElement,
): DOMRect | null {
  const termEl = term.element;
  if (!termEl || term.cols <= 0 || term.rows <= 0) return null;
  const screenEl = termEl.querySelector<HTMLElement>(".xterm-screen");
  const bounds = screenEl?.getBoundingClientRect() ?? hostEl.getBoundingClientRect();
  if (bounds.width <= 0 || bounds.height <= 0) return null;
  return bounds;
}

export function clientToBufferCell(
  term: Terminal,
  clientX: number,
  clientY: number,
  hostEl: HTMLElement,
): BufferCell | null {
  const bounds = screenBounds(term, hostEl);
  if (!bounds) return null;

  const relX = clientX - bounds.left;
  const relY = clientY - bounds.top;
  if (relX < 0 || relY < 0 || relX > bounds.width || relY > bounds.height) return null;

  const cellWidth = bounds.width / term.cols;
  const cellHeight = bounds.height / term.rows;
  const col = Math.min(term.cols - 1, Math.max(0, Math.floor(relX / cellWidth)));
  const rowInView = Math.min(term.rows - 1, Math.max(0, Math.floor(relY / cellHeight)));
  const row = term.buffer.active.viewportY + rowInView;
  return { col, row };
}

export function wordBoundsAtCol(lineStr: string, col: number): { start: number; end: number } | null {
  const trimmed = lineStr.trimEnd();
  if (!trimmed || col >= trimmed.length) return null;

  let start = col;
  while (start > 0 && isWordChar(trimmed[start - 1] ?? "")) start -= 1;
  let end = col;
  while (end < trimmed.length && isWordChar(trimmed[end] ?? "")) end += 1;

  const length = end - start;
  if (length <= 0) return null;
  return { start, end };
}

export function selectionRangeBetweenCells(
  anchorCol: number,
  anchorRow: number,
  currentCol: number,
  currentRow: number,
  cols: number,
): { col: number; row: number; length: number } {
  const anchorOffset = anchorRow * cols + anchorCol;
  const currentOffset = currentRow * cols + currentCol;
  const startOffset = Math.min(anchorOffset, currentOffset);
  const endOffset = Math.max(anchorOffset, currentOffset);
  return {
    row: Math.floor(startOffset / cols),
    col: startOffset % cols,
    length: endOffset - startOffset + 1,
  };
}

export function selectionRangeFromWordAnchor(
  wordStart: number,
  wordEnd: number,
  currentCol: number,
  currentRow: number,
  anchorRow: number,
  cols: number,
): { col: number; row: number; length: number } {
  const wordEndCol = wordEnd - 1;
  const spanStart = Math.min(wordStart, currentCol);
  const spanEnd = Math.max(wordEndCol, currentCol);
  return selectionRangeBetweenCells(spanStart, anchorRow, spanEnd, currentRow, cols);
}

export function selectRangeFromWordAnchor(
  term: Terminal,
  wordStart: number,
  wordEnd: number,
  currentCol: number,
  currentRow: number,
  anchorRow: number,
): void {
  if (term.cols <= 0) return;
  const { col, row, length } = selectionRangeFromWordAnchor(
    wordStart,
    wordEnd,
    currentCol,
    currentRow,
    anchorRow,
    term.cols,
  );
  term.select(col, row, length);
}

export function selectRangeBetweenCells(
  term: Terminal,
  anchorCol: number,
  anchorRow: number,
  currentCol: number,
  currentRow: number,
): void {
  if (term.cols <= 0) return;
  const { col, row, length } = selectionRangeBetweenCells(
    anchorCol,
    anchorRow,
    currentCol,
    currentRow,
    term.cols,
  );
  term.select(col, row, length);
}

export function selectWordAtClient(
  term: Terminal,
  clientX: number,
  clientY: number,
  hostEl: HTMLElement,
): boolean {
  const cell = clientToBufferCell(term, clientX, clientY, hostEl);
  if (!cell) return false;

  const line = term.buffer.active.getLine(cell.row);
  if (!line) return false;

  const bounds = wordBoundsAtCol(line.translateToString(false), cell.col);
  if (!bounds) return false;

  term.select(bounds.start, cell.row, bounds.end - bounds.start);
  return true;
}
