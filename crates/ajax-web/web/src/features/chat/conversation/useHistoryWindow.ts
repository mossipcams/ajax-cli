import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ConversationItem } from "../session/public";
import {
  DEFAULT_HISTORY_WINDOW,
  HISTORY_REVEAL_BATCH,
  historyWindowStart,
  snapRevealStart,
} from "./historyWindow";

export function useHistoryWindow(items: ConversationItem[], sessionKey: string) {
  const [windowStart, setWindowStart] = useState(0);
  const [windowGeneration, setWindowGeneration] = useState(0);
  const anchorLenRef = useRef<number | null>(null);
  const prevSessionKeyRef = useRef(sessionKey);

  const len = items.length;
  const sessionChanged = prevSessionKeyRef.current !== sessionKey;
  const anchorLen = anchorLenRef.current;
  const needsRecompute =
    sessionChanged ||
    anchorLen === null ||
    len === 0 ||
    (anchorLen !== null && len < anchorLen);

  const effectiveWindowStart =
    !needsRecompute
      ? windowStart
      : len === 0
        ? 0
        : historyWindowStart(items, DEFAULT_HISTORY_WINDOW);

  useLayoutEffect(() => {
    if (prevSessionKeyRef.current === sessionKey) return;
    prevSessionKeyRef.current = sessionKey;
    anchorLenRef.current = null;
    setWindowStart(0);
    setWindowGeneration(0);
  }, [sessionKey]);

  useLayoutEffect(() => {
    const len = items.length;

    if (len === 0 || (anchorLenRef.current !== null && len < anchorLenRef.current)) {
      anchorLenRef.current = null;
    }

    if (anchorLenRef.current === null) {
      if (len === 0) {
        setWindowStart(0);
        return;
      }
      setWindowStart(historyWindowStart(items, DEFAULT_HISTORY_WINDOW));
      anchorLenRef.current = len;
      return;
    }

    if (len !== anchorLenRef.current) {
      anchorLenRef.current = len;
    }
  }, [items, sessionKey]);

  const visibleItems = useMemo(
    () => items.slice(effectiveWindowStart),
    [items, effectiveWindowStart],
  );
  const hasEarlier = effectiveWindowStart > 0;

  const revealEarlier = useCallback(() => {
    if (effectiveWindowStart === 0) return 0;
    const nextStart = snapRevealStart(items, effectiveWindowStart, HISTORY_REVEAL_BATCH);
    const revealed = effectiveWindowStart - nextStart;
    if (revealed <= 0) return 0;
    setWindowStart(nextStart);
    setWindowGeneration((generation) => generation + 1);
    return revealed;
  }, [items, effectiveWindowStart]);

  return {
    visibleItems,
    windowStart: effectiveWindowStart,
    hasEarlier,
    revealEarlier,
    windowGeneration,
  };
}
