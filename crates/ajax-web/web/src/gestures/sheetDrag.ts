// Pure drag-to-dismiss math for the bottom sheet. The component owns the touch
// listeners and the transform; this owns only the threshold decision.

export const SHEET_DISMISS_THRESHOLD = 96; // px downward drag to dismiss

export interface SheetDragState {
  /** Downward translate in px (never negative — the sheet can't drag up). */
  offset: number;
}

export function sheetStart(): SheetDragState {
  return { offset: 0 };
}

export function sheetMove(_state: SheetDragState, dy: number): SheetDragState {
  return { offset: Math.max(0, dy) };
}

export function sheetEnd(state: SheetDragState): { dismiss: boolean; offset: number } {
  const dismiss = state.offset >= SHEET_DISMISS_THRESHOLD;
  return { dismiss, offset: dismiss ? state.offset : 0 };
}
