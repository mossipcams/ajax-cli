import { useEffect, useRef, type RefObject } from "react";
import {
  SESSION_PIN_THRESHOLD_PX,
  claimSessionViewportOwnership,
  releaseSessionViewportOwnership,
  sessionSurfaceStyle,
} from "@/shared/lib/sessionViewport";
import { useMobileKeyboard } from "./useMobileKeyboard";

interface Options {
  threadRef: RefObject<HTMLDivElement | null>;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  pinnedRef: RefObject<boolean>;
}

function isAtLiveBottom(node: HTMLDivElement): boolean {
  return node.scrollHeight - node.scrollTop - node.clientHeight < SESSION_PIN_THRESHOLD_PX;
}

/** Repin to scrollHeight for ~500ms while flex layout settles after a chrome resize. */
function repinLiveEdge(node: HTMLDivElement): () => void {
  let raf = 0;
  const start = performance.now();
  const pin = () => {
    node.scrollTop = node.scrollHeight;
    if (performance.now() - start < 500) raf = requestAnimationFrame(pin);
  };
  raf = requestAnimationFrame(pin);
  return () => cancelAnimationFrame(raf);
}

/**
 * Session chat viewport ownership: claim geometry from global keyboard pin,
 * apply iOS Safari bottom reservation, and preserve live-edge vs history scroll
 * across keyboard and composer height changes.
 */
export function useSessionChatViewport({ threadRef, composerRef, pinnedRef }: Options) {
  const { keyboardOpen, keyboardHeight, innerHeight, visualViewportHeight } = useMobileKeyboard();
  const lastAtBottomRef = useRef(false);
  const lastAtBottomAtRef = useRef(0);
  const savedScrollTopRef = useRef<number | null>(null);
  const keyboardTransitionInitRef = useRef(true);
  const composerHeightRef = useRef(0);

  useEffect(() => {
    claimSessionViewportOwnership();
    return () => releaseSessionViewportOwnership();
  }, []);

  // Sample live-edge intent from scroll events (keyboard resize is not a scroll-up).
  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    const onScroll = () => {
      const atBottom = isAtLiveBottom(node);
      lastAtBottomRef.current = atBottom;
      if (atBottom) {
        lastAtBottomAtRef.current = performance.now();
        savedScrollTopRef.current = null;
      } else {
        savedScrollTopRef.current = node.scrollTop;
      }
    };
    node.addEventListener("scroll", onScroll, { passive: true });
    return () => node.removeEventListener("scroll", onScroll);
  }, [threadRef]);

  // Hold bottom pin across keyboard open/close when the operator was at the live edge.
  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    if (keyboardTransitionInitRef.current) {
      keyboardTransitionInitRef.current = false;
      return;
    }

    const recentlyAtBottom =
      lastAtBottomAtRef.current > 0 &&
      performance.now() - lastAtBottomAtRef.current < 1200;
    const shouldRepin =
      pinnedRef.current || lastAtBottomRef.current || recentlyAtBottom;

    if (shouldRepin) {
      savedScrollTopRef.current = null;
      return repinLiveEdge(node);
    }

    const target = savedScrollTopRef.current ?? node.scrollTop;
    let raf = 0;
    raf = requestAnimationFrame(() => {
      node.scrollTop = target;
    });
    return () => cancelAnimationFrame(raf);
  }, [keyboardOpen, pinnedRef, threadRef]);

  // Composer growth while pinned uses the same repin model as the keyboard band.
  useEffect(() => {
    const composer = composerRef.current;
    const node = threadRef.current;
    if (!composer || !node || typeof ResizeObserver === "undefined") return;

    composerHeightRef.current = composer.offsetHeight;
    const observer = new ResizeObserver(() => {
      const nextHeight = composer.offsetHeight;
      if (nextHeight === composerHeightRef.current) return;
      composerHeightRef.current = nextHeight;

      if (!pinnedRef.current && !lastAtBottomRef.current) return;
      repinLiveEdge(node);
    });
    observer.observe(composer);
    return () => observer.disconnect();
  }, [composerRef, pinnedRef, threadRef]);

  const surfaceStyle = sessionSurfaceStyle(
    innerHeight,
    visualViewportHeight,
    keyboardOpen,
  );

  return { keyboardOpen, keyboardHeight, surfaceStyle };
}
