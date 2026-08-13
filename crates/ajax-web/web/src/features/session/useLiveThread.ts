import { useCallback, useEffect, useRef, useState, type UIEvent } from "react";

const PIN_THRESHOLD_PX = 48;

export function useLiveThread(
  entries: unknown,
  toolCount: number,
  observeKey: string | null,
) {
  const threadRef = useRef<HTMLDivElement | null>(null);
  const seenRef = useRef({ entries, toolCount });
  const pinnedRef = useRef(true);
  const [pinned, setPinned] = useState(true);
  const [behind, setBehind] = useState(false);

  pinnedRef.current = pinned;

  const scrollToLive = useCallback(() => {
    const node = threadRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
    setPinned(true);
    setBehind(false);
  }, []);

  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    if (pinned) {
      node.scrollTop = node.scrollHeight;
      seenRef.current = { entries, toolCount };
      return;
    }
    if (entries !== seenRef.current.entries || toolCount !== seenRef.current.toolCount) {
      setBehind(true);
    }
  }, [entries, toolCount, pinned]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (pinnedRef.current) node.scrollTop = node.scrollHeight;
    });
    observer.observe(node);
    for (const child of node.children) observer.observe(child);
    return () => observer.disconnect();
  }, [observeKey, entries]);

  function onScroll(event: UIEvent<HTMLDivElement>) {
    const node = event.currentTarget;
    const atLive = node.scrollHeight - node.scrollTop - node.clientHeight < PIN_THRESHOLD_PX;
    setPinned(atLive);
    if (atLive) setBehind(false);
  }

  const unseenTools = Math.max(0, toolCount - (pinned ? toolCount : seenRef.current.toolCount));

  return { threadRef, behind, unseenTools, onScroll, scrollToLive };
}
