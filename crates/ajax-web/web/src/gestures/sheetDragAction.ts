// Svelte action bridging touch events to the pure sheet drag-to-dismiss state.
// Logic lives in ./sheetDrag so it stays unit-testable.

import { sheetStart, sheetMove, sheetEnd, type SheetDragState } from "./sheetDrag";

export interface SheetDragOptions {
  onDismiss: () => void;
  onOffset?: (offset: number) => void;
}

function readTouchY(event: Event): number | null {
  const touches = (event as TouchEvent).touches;
  if (!touches || touches.length !== 1) return null;
  return touches[0].clientY;
}

export function sheetDrag(node: HTMLElement, options: SheetDragOptions) {
  let opts = options;
  let state: SheetDragState | null = null;
  let startY = 0;

  const onStart = (event: Event) => {
    const y = readTouchY(event);
    if (y === null) return;
    startY = y;
    state = sheetStart();
  };

  const onMove = (event: Event) => {
    if (!state) return;
    const y = readTouchY(event);
    if (y === null) return;
    state = sheetMove(state, y - startY);
    opts.onOffset?.(state.offset);
  };

  const onEnd = () => {
    if (!state) return;
    const settled = sheetEnd(state);
    opts.onOffset?.(settled.offset);
    state = null;
    if (settled.dismiss) opts.onDismiss();
  };

  node.addEventListener("touchstart", onStart, { passive: true });
  node.addEventListener("touchmove", onMove, { passive: true });
  node.addEventListener("touchend", onEnd);
  node.addEventListener("touchcancel", onEnd);

  return {
    update(next: SheetDragOptions) {
      opts = next;
    },
    destroy() {
      node.removeEventListener("touchstart", onStart);
      node.removeEventListener("touchmove", onMove);
      node.removeEventListener("touchend", onEnd);
      node.removeEventListener("touchcancel", onEnd);
    },
  };
}
