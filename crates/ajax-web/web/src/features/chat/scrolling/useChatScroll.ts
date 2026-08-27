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
  afterTranscriptLayoutSettles,
  captureTranscriptGeometry,
  pinTranscriptToLiveEdge,
  restoreTranscriptGeometry,
  transcriptAtLiveEdge,
} from "@/shared/lib/sessionViewport";
import {
  anchorIsStale,
  autoLoadDecision,
  restoreScrollAfterTopGrowth,
} from "./historyScroll";

/** Re-export for callers/tests that imported the hook-local constant. */
export const PIN_THRESHOLD_PX = SESSION_PIN_THRESHOLD_PX;

export interface HistoryScrollControl {
  hasEarlier: boolean;
  revealEarlier: () => number;
  windowGeneration: number;
}

interface Options {
  threadRef: RefObject<HTMLDivElement | null>;
  /** Session projection revision — content changed when this increments. */
  revision: number;
  /** Re-subscribe layout observers when the mounted session identity changes. */
  sessionKey: string;
  /** While true, keyboard/viewport layout settle owns scroll — do not re-pin. */
  layoutTransitionRef?: RefObject<boolean>;
  historyScroll?: HistoryScrollControl;
}

function liveEdgeTarget(node: HTMLDivElement) {
  return captureTranscriptGeometry(node);
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
  const autoLoadRef = useRef({ armed: false, lastLoadAt: 0 });
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

  const scrollToLiveEdge = useCallback((node: HTMLDivElement, settle = false) => {
    const target = liveEdgeTarget(node);
    ignoreScrollIntentRef.current = true;
    pinTranscriptToLiveEdge(node);
    ignoreScrollIntentRef.current = false;
    if (!settle) {
      return () => {};
    }
    return afterTranscriptLayoutSettles(
      node,
      { ...target, atBottom: true },
      () => {
        ignoreScrollIntentRef.current = true;
        restoreTranscriptGeometry(node, { ...target, atBottom: true });
        ignoreScrollIntentRef.current = false;
      },
      { ignoreProgrammaticScroll: ignoreScrollIntentRef, pinnedRef },
    );
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
    if (revealed <= 0) return 0;

    pendingTopRestoreRef.current = { ...before, revealed };
    return revealed;
  }, [historyScroll, threadRef]);

  // Restore read position after prepend-style window growth.
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

  // Session identity is separate from pin state so setPinned(true) here does not
  // re-run this effect and cancel the layout-settle poll (#1065).
  useLayoutEffect(() => {
    const node = threadRef.current;
    if (!node || prevSessionKeyRef.current === sessionKey) return;

    prevSessionKeyRef.current = sessionKey;
    setPinned(true);
    setBehind(false);
    pinnedRef.current = true;
    seenRevisionRef.current = revisionRef.current;
    contentScrollHeightRef.current = node.scrollHeight;
    autoLoadRef.current = { armed: false, lastLoadAt: 0 };
    prevWindowGenerationRef.current = historyScroll?.windowGeneration ?? 0;
    pendingTopRestoreRef.current = null;
    return scrollToLiveEdge(node, true);
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
