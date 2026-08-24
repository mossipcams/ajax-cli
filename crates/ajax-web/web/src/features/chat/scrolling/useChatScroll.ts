import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
  type UIEvent,
} from "react";

/** Treat "within this many px of the bottom" as following the live edge. */
export const PIN_THRESHOLD_PX = 48;

interface Options {
  threadRef: RefObject<HTMLDivElement | null>;
  /** Session projection revision — content changed when this increments. */
  revision: number;
  /** Re-subscribe layout observers when the mounted session identity changes. */
  sessionKey: string;
}

export function useChatScroll({ threadRef, revision, sessionKey }: Options) {
  const seenRevisionRef = useRef(0);
  const pinnedRef = useRef(true);

  const [pinned, setPinned] = useState(true);
  const [behind, setBehind] = useState(false);

  pinnedRef.current = pinned;

  const restoreLiveEdge = useCallback(() => {
    setPinned(true);
    setBehind(false);
  }, []);

  const scrollToLatest = useCallback(() => {
    const node = threadRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
    setPinned(true);
    setBehind(false);
  }, [threadRef]);

  useLayoutEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    if (pinned) {
      node.scrollTop = node.scrollHeight;
      seenRevisionRef.current = revision;
      return;
    }
    if (revision !== seenRevisionRef.current) setBehind(true);
  }, [revision, pinned, threadRef]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => {
      if (!pinnedRef.current) return;
      node.scrollTop = node.scrollHeight;
    });
    observer.observe(node, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [sessionKey, threadRef]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (!pinnedRef.current) return;
      node.scrollTop = node.scrollHeight;
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [sessionKey, threadRef]);

  function onThreadScroll(event: UIEvent<HTMLDivElement>) {
    const node = event.currentTarget;
    const atLive = node.scrollHeight - node.scrollTop - node.clientHeight < PIN_THRESHOLD_PX;
    setPinned(atLive);
    // Away from the live edge is the whole condition. Waiting for new content
    // to arrive first left a settled transcript with no way back down but a
    // long drag on a phone.
    setBehind(!atLive);
  }

  return {
    pinnedRef,
    behind,
    scrollToLatest,
    restoreLiveEdge,
    onThreadScroll,
  };
}
