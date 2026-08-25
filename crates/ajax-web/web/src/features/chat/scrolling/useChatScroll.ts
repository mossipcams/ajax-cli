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
  afterTranscriptLayoutSettles,
  captureTranscriptGeometry,
  restoreTranscriptGeometry,
} from "@/shared/lib/sessionViewport";

/** Treat "within this many px of the bottom" as following the live edge. */
export const PIN_THRESHOLD_PX = 48;

interface Options {
  threadRef: RefObject<HTMLDivElement | null>;
  /** Session projection revision — content changed when this increments. */
  revision: number;
  /** Re-subscribe layout observers when the mounted session identity changes. */
  sessionKey: string;
  /** While true, keyboard/viewport layout settle owns scroll — do not re-pin. */
  layoutTransitionRef?: RefObject<boolean>;
}

function liveEdgeTarget(node: HTMLDivElement) {
  return { ...captureTranscriptGeometry(node), atBottom: true };
}

export function useChatScroll({ threadRef, revision, sessionKey, layoutTransitionRef }: Options) {
  const seenRevisionRef = useRef(0);
  const prevSessionKeyRef = useRef(sessionKey);
  const revisionRef = useRef(revision);
  const pinnedRef = useRef(true);
  const ignoreScrollIntentRef = useRef(false);

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
    node.scrollTop = node.scrollHeight;
    ignoreScrollIntentRef.current = false;
    if (!settle) {
      return () => {};
    }
    return afterTranscriptLayoutSettles(
      node,
      target,
      () => {
        ignoreScrollIntentRef.current = true;
        restoreTranscriptGeometry(node, target);
        ignoreScrollIntentRef.current = false;
      },
      { ignoreProgrammaticScroll: ignoreScrollIntentRef },
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
    return scrollToLiveEdge(node, true);
  }, [sessionKey, scrollToLiveEdge, threadRef]);

  useLayoutEffect(() => {
    const node = threadRef.current;
    if (!node) return;

    if (!pinned) {
      if (revision !== seenRevisionRef.current) setBehind(true);
      return;
    }

    if (revision === seenRevisionRef.current) return;

    scrollToLiveEdge(node);
    seenRevisionRef.current = revision;
  }, [revision, pinned, scrollToLiveEdge, threadRef]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => {
      if (layoutTransitionRef?.current || !pinnedRef.current) return;
      scrollToLiveEdge(node);
    });
    observer.observe(node, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [sessionKey, scrollToLiveEdge, threadRef]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (layoutTransitionRef?.current || !pinnedRef.current) return;
      scrollToLiveEdge(node);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [sessionKey, scrollToLiveEdge, threadRef]);

  function onThreadScroll(event: UIEvent<HTMLDivElement>) {
    if (ignoreScrollIntentRef.current || layoutTransitionRef?.current) return;
    const node = event.currentTarget;
    const atLive = node.scrollHeight - node.scrollTop - node.clientHeight < PIN_THRESHOLD_PX;
    pinnedRef.current = atLive;
    setPinned(atLive);
    // Away from the live edge is the whole condition. Waiting for new content
    // to arrive first left a settled transcript with no way back down but a
    // long drag on a phone.
    setBehind(!atLive);
  }

  return {
    pinnedRef,
    ignoreScrollIntentRef,
    behind,
    scrollToLatest,
    restoreLiveEdge,
    onThreadScroll,
  };
}
