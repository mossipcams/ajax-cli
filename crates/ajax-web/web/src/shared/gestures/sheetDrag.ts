export const SHEET_DISMISS_THRESHOLD = 96;

export interface SheetDragState {
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
