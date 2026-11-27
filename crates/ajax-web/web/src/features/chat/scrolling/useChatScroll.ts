import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
  type UIEvent,
} from "react";
import {
  SESSION_PIN_THRESHOLD_PX,
  pinTranscriptToLiveEdge,
  transcriptAtLiveEdge,
  transcriptScrollBottom,
} from "@/shared/lib/sessionViewport";
import {
  anchorIsStale,
  autoLoadDecision,
  restoreScrollAfterTopGrowth,
} from "./historyScroll";

export const PIN_THRESHOLD_PX = SESSION_PIN_THRESHOLD_PX;

const readingPositions = new Map<string, number>();

export function resetReadingPositionsForTests(): void {
  readingPositions.clear();
}

export interface HistoryScrollControl {
  hasEarlier: boolean;
  revealEarlier: () => number;
  windowGeneration: number;
}

interface Options {
  threadRef: RefObject<HTMLDivElement | null>;
  revision: number;
  sessionKey: string;
  layoutTransitionRef?: RefObject<boolean>;
  historyScroll?: HistoryScrollControl;
}

export function useChatScroll({
  threadRef,
  revision,
  sessionKey,
  layoutTransitionRef,
  historyScroll,
}: Options) {
  const seenRevisionRef = useRef(0);
  const prevSessionKeyRef = useRef<string | null>(null);
  const revisionRef = useRef(revision);
  const pinnedRef = useRef(true);
  const ignoreScrollIntentRef = useRef(false);
  const contentScrollHeightRef = useRef(0);
  const autoLoadRef = useRef({ armed: true, lastLoadAt: 0 });
  const prevWindowGenerationRef = useRef(0);
  const pendingReadingRef = useRef<number | null>(null);
  const settlingAfterRestoreRef = useRef(false);
  const pendingTopRestoreRef = useRef<{
    scrollTop: number;
    scrollHeight: number;
    revealed: number;
  } | null>(null);

  const [pinned, setPinned] = useState(true);
  const [behind, setBehind] = useState(false);

  revisionRef.current = revision;
  pinnedRef.current = pinned;

  const applyPendingReadingRestore = useCallback((node: HTMLDivElement) => {
    const distance = pendingReadingRef.current;
    if (distance === null) return;
    const maxScroll = Math.max(0, node.scrollHeight - node.clientHeight);
    if (maxScroll < distance) return;
    pendingReadingRef.current = null;
    ignoreScrollIntentRef.current = true;
    node.scrollTop = -distance;
    ignoreScrollIntentRef.current = false;
    settlingAfterRestoreRef.current = true;
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        settlingAfterRestoreRef.current = false;
      }),
    );
    const atLive = transcriptAtLiveEdge(node, PIN_THRESHOLD_PX);
    pinnedRef.current = atLive;
    setPinned(atLive);
    setBehind(!atLive);
  }, []);

  const restoreLiveEdge = useCallback(() => {
    readingPositions.delete(sessionKey);
    pendingReadingRef.current = null;
    setPinned(true);
    setBehind(false);
    pinnedRef.current = true;
  }, [sessionKey]);

  const scrollToLiveEdge = useCallback((node: HTMLDivElement) => {
    ignoreScrollIntentRef.current = true;
    pinTranscriptToLiveEdge(node);
    ignoreScrollIntentRef.current = false;
  }, []);

  const scrollToLatest = useCallback(() => {
    const node = threadRef.current;
    if (!node) return;
    readingPositions.delete(sessionKey);
    pendingReadingRef.current = null;
    scrollToLiveEdge(node);
    setPinned(true);
    setBehind(false);
    pinnedRef.current = true;
  }, [scrollToLiveEdge, sessionKey, threadRef]);

  const loadEarlier = useCallback(() => {
    const node = threadRef.current;
    if (!node || !historyScroll?.hasEarlier) return 0;

    const before = {
      scrollTop: node.scrollTop,
      scrollHeight: node.scrollHeight,
    };
    const revealed = historyScroll.revealEarlier();
    if (revealed <= 0) {
      pendingTopRestoreRef.current = { ...before, revealed: 1 };
      return 0;
    }

    pendingTopRestoreRef.current = { ...before, revealed };
    return revealed;
  }, [historyScroll, threadRef]);

  useLayoutEffect(() => {
    const generation = historyScroll?.windowGeneration ?? 0;
    if (generation === prevWindowGenerationRef.current) return;
    prevWindowGenerationRef.current = generation;

    const node = threadRef.current;
    const pending = pendingTopRestoreRef.current;
    if (!node || !pending) {
      pendingTopRestoreRef.current = null;
      return;
    }

    if (anchorIsStale(pending.scrollHeight, node.scrollHeight, pending.revealed)) {
      pendingTopRestoreRef.current = null;
      return;
    }
    ignoreScrollIntentRef.current = true;
    restoreScrollAfterTopGrowth(node, pending.scrollTop, pending.scrollHeight);
    ignoreScrollIntentRef.current = false;
    pendingTopRestoreRef.current = null;

    const atLive = transcriptAtLiveEdge(node, PIN_THRESHOLD_PX);
    pinnedRef.current = atLive;
    setPinned(atLive);
    setBehind(!atLive);
  }, [historyScroll?.windowGeneration, threadRef]);

  const loadEarlierRef = useRef(loadEarlier);
  loadEarlierRef.current = loadEarlier;

  const fillHasEarlier = historyScroll?.hasEarlier ?? false;
  useLayoutEffect(() => {
    const node = threadRef.current;
    if (!node || !fillHasEarlier) return;
    if (pendingTopRestoreRef.current) return;
    if (node.clientHeight <= 0 || node.scrollHeight > node.clientHeight) return;
    loadEarlierRef.current();
  }, [fillHasEarlier, historyScroll?.windowGeneration, threadRef]);

  useLayoutEffect(() => {
    const node = threadRef.current;
    if (!node || prevSessionKeyRef.current === sessionKey) return;

    const isMount = prevSessionKeyRef.current === null;
    const savedDistance = isMount ? readingPositions.get(sessionKey) : undefined;
    prevSessionKeyRef.current = sessionKey;
    seenRevisionRef.current = revisionRef.current;
    contentScrollHeightRef.current = node.scrollHeight;
    autoLoadRef.current = { armed: true, lastLoadAt: 0 };
    prevWindowGenerationRef.current = historyScroll?.windowGeneration ?? 0;
    pendingTopRestoreRef.current = null;
    pendingReadingRef.current = null;

    if (savedDistance === undefined || node.clientHeight <= 0) {
      setPinned(true);
      setBehind(false);
      pinnedRef.current = true;
      scrollToLiveEdge(node);
      return;
    }

    pendingReadingRef.current = savedDistance;
    pinnedRef.current = false;
    setPinned(false);
    setBehind(true);
    applyPendingReadingRestore(node);
  }, [sessionKey, scrollToLiveEdge, applyPendingReadingRestore, threadRef]);

  useLayoutEffect(() => {
    const node = threadRef.current;
    if (!node) return;

    if (!pinnedRef.current) {
      if (revision !== seenRevisionRef.current) setBehind(true);
      return;
    }

    if (revision === seenRevisionRef.current) return;

    pinTranscriptToLiveEdge(node);
    seenRevisionRef.current = revision;
    contentScrollHeightRef.current = node.scrollHeight;
  }, [revision, pinned, threadRef]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => {
      if (layoutTransitionRef?.current) return;
      if (pendingTopRestoreRef.current) return;
      if (pendingReadingRef.current !== null) {
        applyPendingReadingRestore(node);
        return;
      }
      if (pinnedRef.current) {
        pinTranscriptToLiveEdge(node);
        contentScrollHeightRef.current = node.scrollHeight;
        return;
      }
    });
    observer.observe(node, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [sessionKey, threadRef, layoutTransitionRef, applyPendingReadingRestore]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;

    contentScrollHeightRef.current = node.scrollHeight;
    let lastTop = node.getBoundingClientRect().top;
    const observer = new ResizeObserver(() => {
      const top = node.getBoundingClientRect().top;
      const delta = top - lastTop;
      lastTop = top;
      if (layoutTransitionRef?.current) return;
      if (pendingTopRestoreRef.current) return;
      if (pendingReadingRef.current !== null) {
        applyPendingReadingRestore(node);
        return;
      }
      if (settlingAfterRestoreRef.current) return;
      if (pinnedRef.current) {
        pinTranscriptToLiveEdge(node);
        contentScrollHeightRef.current = node.scrollHeight;
      } else if (delta !== 0) {
        node.scrollTop += delta;
      }
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [sessionKey, threadRef, layoutTransitionRef, applyPendingReadingRestore]);

  function onThreadScroll(event: UIEvent<HTMLDivElement>) {
    if (ignoreScrollIntentRef.current || layoutTransitionRef?.current) return;
    const node = event.currentTarget;
    const atLive = transcriptAtLiveEdge(node, PIN_THRESHOLD_PX);
    if (pendingReadingRef.current !== null) {
      if (atLive) return;
      pendingReadingRef.current = null;
    }
    if (atLive) {
      readingPositions.delete(sessionKey);
    } else {
      readingPositions.set(
        sessionKey,
        transcriptScrollBottom(node.scrollTop, node.scrollHeight, node.clientHeight),
      );
    }
    pinnedRef.current = atLive;
    setPinned(atLive);
    setBehind(!atLive);

    if (!historyScroll) return;

    const decision = autoLoadDecision(node, autoLoadRef.current, historyScroll.hasEarlier, Date.now());
    autoLoadRef.current = decision.nextState;
    if (decision.shouldLoad) loadEarlier();
  }

  return {
    pinnedRef,
    ignoreScrollIntentRef,
    behind,
    scrollToLatest,
    restoreLiveEdge,
    onThreadScroll,
    loadEarlier,
    hasEarlier: historyScroll?.hasEarlier ?? false,
  };
}
