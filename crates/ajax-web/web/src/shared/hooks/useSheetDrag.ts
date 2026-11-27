import { useEffect, useRef, type RefObject } from "react";
import { RELEASE_VELOCITY_WINDOW_MS } from "@/shared/gestures/navigateSwipe";
import {
  sheetDismisses,
  sheetOffset,
  sheetReleaseVelocity,
  type SheetSample,
} from "@/shared/gestures/sheetDrag";

export interface SheetDragOptions {
  onDismiss: () => void;
  target: RefObject<HTMLElement | null>;
  enabled?: boolean;
}

function readTouchY(event: Event): number | null {
  const touches = (event as TouchEvent).touches;
  if (!touches || touches.length !== 1) return null;
  return touches[0].clientY;
}

function eventTime(event: Event): number {
  return event.timeStamp || performance.now();
}

export function useSheetDrag(
  ref: RefObject<HTMLElement | null>,
  options: SheetDragOptions,
): void {
  const optsRef = useRef(options);
  optsRef.current = options;
  const enabled = options.enabled ?? true;

  useEffect(() => {
    const node = ref.current;
    if (!enabled || !node) return;

    let active = false;
    let startY = 0;
    let offset = 0;
    let samples: SheetSample[] = [];
    let frame: number | null = null;

    const paint = () => {
      frame = null;
      const target = optsRef.current.target.current;
      if (target) target.style.transform = `translateY(${offset}px)`;
    };

    const cancelFrame = () => {
      if (frame === null) return;
      cancelAnimationFrame(frame);
      frame = null;
    };

    const onStart = (event: Event) => {
      const y = readTouchY(event);
      if (y === null) return;
      const target = optsRef.current.target.current;
      if (target) {
        target.style.animation = "none";
        target.classList.add("is-dragging");
      }
      active = true;
      startY = y;
      offset = 0;
      samples = [{ t: eventTime(event), y }];
    };

    const onMove = (event: Event) => {
      if (!active) return;
      const y = readTouchY(event);
      if (y === null) return;
      const t = eventTime(event);
      samples = samples.filter((sample) => sample.t >= t - RELEASE_VELOCITY_WINDOW_MS).concat({ t, y });
      offset = sheetOffset(y - startY);
      if (frame === null) frame = requestAnimationFrame(paint);
    };

    const onEnd = (event: Event) => {
      if (!active) return;
      active = false;
      cancelFrame();
      const target = optsRef.current.target.current;
      if (sheetDismisses(offset, sheetReleaseVelocity(samples, eventTime(event)))) {
        paint();
        target?.classList.remove("is-dragging");
        optsRef.current.onDismiss();
        return;
      }
      if (target) {
        target.classList.remove("is-dragging");
        target.style.transform = "translateY(0px)";
      }
    };

    node.addEventListener("touchstart", onStart, { passive: true });
    node.addEventListener("touchmove", onMove, { passive: true });
    node.addEventListener("touchend", onEnd);
    node.addEventListener("touchcancel", onEnd);

    return () => {
      active = false;
      cancelFrame();
      node.removeEventListener("touchstart", onStart);
      node.removeEventListener("touchmove", onMove);
      node.removeEventListener("touchend", onEnd);
      node.removeEventListener("touchcancel", onEnd);
    };
  }, [ref, enabled]);
}
