import { useEffect, useRef, type RefObject } from "react";
import {
  captureTranscriptGeometry,
  claimSessionViewportOwnership,
  releaseSessionViewportOwnership,
  restoreTranscriptGeometry,
  sessionSurfaceStyle,
  type TranscriptGeometry,
} from "@/shared/lib/sessionViewport";
import { useMobileKeyboard } from "@/shared/hooks/useMobileKeyboard";

interface Options {
  threadRef: RefObject<HTMLDivElement | null>;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  pinnedRef: RefObject<boolean>;
  onRestoreLiveEdge?: () => void;
}

const LAYOUT_STABLE_FRAMES = 2;
const LAYOUT_POLL_MAX_FRAMES = 20;

function layoutKey(node: HTMLDivElement): string {
  return `${node.scrollHeight}:${node.clientHeight}`;
}

/** Poll until scrollHeight/clientHeight stop changing, then run restore once. */
function afterLayoutSettles(
  node: HTMLDivElement,
  restoreTarget: TranscriptGeometry,
  restore: () => void,
): () => void {
  let raf = 0;
  let stableFrames = 0;
  let lastKey = layoutKey(node);
  let frameCount = 0;

  const poll = () => {
    frameCount++;
    if (restoreTarget.atBottom) {
      node.scrollTop = node.scrollHeight;
    }
    const key = layoutKey(node);
    if (key === lastKey) {
      stableFrames++;
    } else {
      stableFrames = 0;
      lastKey = key;
    }

    if (stableFrames >= LAYOUT_STABLE_FRAMES || frameCount >= LAYOUT_POLL_MAX_FRAMES) {
      restore();
      return;
    }
    raf = requestAnimationFrame(poll);
  };

  raf = requestAnimationFrame(poll);
  return () => cancelAnimationFrame(raf);
}

/**
 * Chat viewport ownership: claim geometry from global keyboard pin,
 * apply iOS Safari bottom reservation, and preserve live-edge vs history scroll
 * across keyboard and composer height changes.
 */
export function useChatViewport({
  threadRef,
  composerRef,
  pinnedRef,
  onRestoreLiveEdge,
}: Options) {
  const { keyboardOpen, keyboardHeight, innerHeight, visualViewportHeight } = useMobileKeyboard();
  const geometryRef = useRef<TranscriptGeometry | null>(null);
  const ignoreScrollIntentRef = useRef(false);
  const keyboardTransitionInitRef = useRef(true);
  const prevKeyboardOpenRef = useRef<boolean | null>(null);
  const composerHeightRef = useRef(0);

  useEffect(() => {
    claimSessionViewportOwnership();
    return () => releaseSessionViewportOwnership();
  }, []);

  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;

    geometryRef.current = captureTranscriptGeometry(node);
    const onScroll = () => {
      if (ignoreScrollIntentRef.current) return;
      geometryRef.current = captureTranscriptGeometry(node);
    };
    node.addEventListener("scroll", onScroll, { passive: true });
    return () => node.removeEventListener("scroll", onScroll);
  }, [threadRef]);

  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;

    if (keyboardTransitionInitRef.current) {
      keyboardTransitionInitRef.current = false;
      prevKeyboardOpenRef.current = keyboardOpen;
      geometryRef.current = captureTranscriptGeometry(node);
      return;
    }

    if (prevKeyboardOpenRef.current === keyboardOpen) return;

    const closing = prevKeyboardOpenRef.current === true && !keyboardOpen;
    const before = geometryRef.current ?? captureTranscriptGeometry(node);
    const restoreTarget: TranscriptGeometry = {
      ...before,
      atBottom: before.atBottom || pinnedRef.current,
    };

    prevKeyboardOpenRef.current = keyboardOpen;
    ignoreScrollIntentRef.current = true;

    return afterLayoutSettles(node, restoreTarget, () => {
      restoreTranscriptGeometry(node, restoreTarget);
      if (closing && restoreTarget.atBottom) onRestoreLiveEdge?.();
      geometryRef.current = captureTranscriptGeometry(node);
      ignoreScrollIntentRef.current = false;
    });
  }, [keyboardOpen, onRestoreLiveEdge, pinnedRef, threadRef]);

  useEffect(() => {
    const composer = composerRef.current;
    const node = threadRef.current;
    if (!composer || !node || typeof ResizeObserver === "undefined") return;

    composerHeightRef.current = composer.offsetHeight;
    const observer = new ResizeObserver(() => {
      const nextHeight = composer.offsetHeight;
      if (nextHeight === composerHeightRef.current) return;
      composerHeightRef.current = nextHeight;

      const geo = geometryRef.current ?? captureTranscriptGeometry(node);
      if (!pinnedRef.current && !geo.atBottom) return;

      ignoreScrollIntentRef.current = true;
      restoreTranscriptGeometry(node, { ...geo, atBottom: true });
      geometryRef.current = captureTranscriptGeometry(node);
      ignoreScrollIntentRef.current = false;
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
