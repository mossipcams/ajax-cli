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
} from "@/shared/lib/sessionViewport";
import {
  anchorIsStale,
  autoLoadDecision,
  restoreScrollAfterTopGrowth,
} from "./historyScroll";

export const PIN_THRESHOLD_PX = SESSION_PIN_THRESHOLD_PX;

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
  // Armed on start: the initial pinned live edge is by definition away from
  // the history edge, so a fast scroll that lands directly in the preload band
  // must load without a prior arming pass (the "sometimes doesn't load" flake).
  const autoLoadRef = useRef({ armed: true, lastLoadAt: 0 });
  const prevWindowGenerationRef = useRef(0);
  const pendingTopRestoreRef = useRef<{
    scrollTop: number;
    scrollHeight: number;
    revealed: number;
  } | null>(null);

  const [pinned, setPinned] = useState(true);
  const [behind, setBehind] = useState(false);

  revisionRef.current = revision;
  pinnedRef.current = pinned;

  const restoreLiveEdge = useCallback(() => {
    setPinned(true);
    setBehind(false);
    pinnedRef.current = true;
  }, []);

  // Reversed coords: the live edge is scrollTop = 0, which column-reverse
  // placement gives natively; pinning is the only correction needed.
  const scrollToLiveEdge = useCallback((node: HTMLDivElement) => {
    ignoreScrollIntentRef.current = true;
    pinTranscriptToLiveEdge(node);
    ignoreScrollIntentRef.current = false;
  }, []);

  const scrollToLatest = useCallback(() => {
    const node = threadRef.current;
    if (!node) return;
    scrollToLiveEdge(node);
    setPinned(true);
    setBehind(false);
    pinnedRef.current = true;
  }, [scrollToLiveEdge, threadRef]);

  const loadEarlier = useCallback(() => {
    const node = threadRef.current;
    if (!node || !historyScroll?.hasEarlier) return 0;

    const before = {
      scrollTop: node.scrollTop,
      scrollHeight: node.scrollHeight,
    };
    const revealed = historyScroll.revealEarlier();
    if (revealed <= 0) {
      // Server fetch path: it returns 0 synchronously and grows the thread
      // when its page prepends, so arm the top restore with a sentinel reveal
      // count for that growth. A failed fetch never bumps windowGeneration,
      // so no stale restore can run.
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

  // Stable handle for the fill effect below; loadEarlier rebinds on every
  // historyScroll change, which must not restart the fill loop.
  const loadEarlierRef = useRef(loadEarlier);
  loadEarlierRef.current = loadEarlier;

  // While the rendered tail does not overflow the viewport no scroll event can
  // ever reach the history edge, so scroll-driven loading (armed or manual) is
  // unreachable and a thin transcript leaves whitespace above it. Auto-load
  // older pages until the thread overflows or the server history is exhausted.
  // A successful prepend bumps windowGeneration and re-runs this effect; a
  // failed fetch never does, so the loop stops there and stays retryable via
  // the load-earlier pill.
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

    prevSessionKeyRef.current = sessionKey;
    setPinned(true);
    setBehind(false);
    pinnedRef.current = true;
    seenRevisionRef.current = revisionRef.current;
    contentScrollHeightRef.current = node.scrollHeight;
    autoLoadRef.current = { armed: true, lastLoadAt: 0 };
    prevWindowGenerationRef.current = historyScroll?.windowGeneration ?? 0;
    pendingTopRestoreRef.current = null;
    scrollToLiveEdge(node);
  }, [sessionKey, scrollToLiveEdge, threadRef]);

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
      if (pinnedRef.current) {
        pinTranscriptToLiveEdge(node);
        contentScrollHeightRef.current = node.scrollHeight;
        return;
      }
    });
    observer.observe(node, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [sessionKey, threadRef, layoutTransitionRef]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;

    contentScrollHeightRef.current = node.scrollHeight;
    const observer = new ResizeObserver(() => {
      if (layoutTransitionRef?.current) return;
      if (pendingTopRestoreRef.current) return;
      if (pinnedRef.current) {
        pinTranscriptToLiveEdge(node);
        contentScrollHeightRef.current = node.scrollHeight;
      }
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [sessionKey, threadRef, layoutTransitionRef]);

  function onThreadScroll(event: UIEvent<HTMLDivElement>) {
    if (ignoreScrollIntentRef.current || layoutTransitionRef?.current) return;
    const node = event.currentTarget;
    const atLive = transcriptAtLiveEdge(node, PIN_THRESHOLD_PX);
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
