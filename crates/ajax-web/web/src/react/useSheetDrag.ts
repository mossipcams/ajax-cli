import { useEffect, useRef, type RefObject } from "react";
import { sheetStart, sheetMove, sheetEnd, type SheetDragState } from "../gestures/sheetDrag";

export interface SheetDragOptions {
  onDismiss: () => void;
  onOffset?: (offset: number) => void;
}

function readTouchY(event: Event): number | null {
  const touches = (event as TouchEvent).touches;
  if (!touches || touches.length !== 1) return null;
  return touches[0].clientY;
}

export function useSheetDrag(
  ref: RefObject<HTMLElement | null>,
  options: SheetDragOptions,
): void {
  const optsRef = useRef(options);
  optsRef.current = options;

  useEffect(() => {
    const node = ref.current;
    if (!node) return;

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
      optsRef.current.onOffset?.(state.offset);
    };

    const onEnd = () => {
      if (!state) return;
      const settled = sheetEnd(state);
      optsRef.current.onOffset?.(settled.offset);
      state = null;
      if (settled.dismiss) optsRef.current.onDismiss();
    };

    node.addEventListener("touchstart", onStart, { passive: true });
    node.addEventListener("touchmove", onMove, { passive: true });
    node.addEventListener("touchend", onEnd);
    node.addEventListener("touchcancel", onEnd);

    return () => {
      node.removeEventListener("touchstart", onStart);
      node.removeEventListener("touchmove", onMove);
      node.removeEventListener("touchend", onEnd);
      node.removeEventListener("touchcancel", onEnd);
    };
  }, [ref]);
}
