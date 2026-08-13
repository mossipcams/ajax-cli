import { useEffect, useRef, useState } from "react";

const DRAIN_MS = 250;
const MAX_CHAR_INTERVAL_MS = 5;

function prefersReducedMotion(): boolean {
  if (typeof window === "undefined") return false;
  if (typeof window.matchMedia !== "function") return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function useSmoothText(text: string, enabled: boolean): string {
  const motionReduced = prefersReducedMotion();
  const effectiveEnabled = enabled && !motionReduced;

  const [shown, setShown] = useState(() => (effectiveEnabled ? "" : text));
  const shownRef = useRef(shown);
  const rafRef = useRef<number | null>(null);
  const lastFrameTimeRef = useRef<number | null>(null);
  const prevEnabledRef = useRef(enabled);

  shownRef.current = shown;

  useEffect(() => {
    const reduced = prefersReducedMotion();
    const active = enabled && !reduced;

    if (enabled && !prevEnabledRef.current && !reduced) {
      shownRef.current = "";
      setShown("");
      lastFrameTimeRef.current = null;
    }
    prevEnabledRef.current = enabled;

    const cancelRaf = () => {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
      lastFrameTimeRef.current = null;
    };

    if (!active) {
      cancelRaf();
      if (shownRef.current !== text) {
        shownRef.current = text;
        setShown(text);
      }
      return cancelRaf;
    }

    if (
      shownRef.current.length > 0 &&
      !text.startsWith(shownRef.current)
    ) {
      shownRef.current = "";
      setShown("");
      lastFrameTimeRef.current = null;
    }

    const tick = (time: number) => {
      rafRef.current = null;

      if (!enabled || prefersReducedMotion()) return;

      const target = text;
      let cursor = shownRef.current;

      if (cursor.length >= target.length) {
        lastFrameTimeRef.current = null;
        return;
      }

      const lastTime = lastFrameTimeRef.current;
      const dt = lastTime == null ? 16 : Math.max(0, time - lastTime);
      lastFrameTimeRef.current = time;

      let budget = dt;
      while (budget > 0 && cursor.length < target.length) {
        const remaining = target.length - cursor.length;
        const timePerChar = Math.min(
          MAX_CHAR_INTERVAL_MS,
          DRAIN_MS / remaining,
        );
        if (budget < timePerChar) break;
        budget -= timePerChar;
        cursor = target.slice(0, cursor.length + 1);
      }

      if (cursor !== shownRef.current) {
        shownRef.current = cursor;
        setShown(cursor);
      }

      if (shownRef.current.length < target.length) {
        rafRef.current = requestAnimationFrame(tick);
      } else {
        lastFrameTimeRef.current = null;
      }
    };

    if (shownRef.current.length < text.length) {
      rafRef.current = requestAnimationFrame(tick);
    }

    return cancelRaf;
  }, [text, enabled]);

  if (!effectiveEnabled) return text;
  return shown;
}
