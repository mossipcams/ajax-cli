import { useEffect, useRef, type RefObject } from "react";
import {
  afterTranscriptLayoutSettles,
  captureTranscriptGeometry,
  claimSessionViewportOwnership,
  pinTranscriptToLiveEdge,
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
  ignoreScrollIntentRef: RefObject<boolean>;
  layoutTransitionRef: RefObject<boolean>;
  onRestoreLiveEdge?: () => void;
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
  ignoreScrollIntentRef,
  layoutTransitionRef,
  onRestoreLiveEdge,
}: Options) {
  const { keyboardOpen, keyboardHeight, innerHeight, visualViewportHeight } = useMobileKeyboard();
  const geometryRef = useRef<TranscriptGeometry | null>(null);
  const preKeyboardGeometryRef = useRef<TranscriptGeometry | null>(null);
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
  }, [threadRef, ignoreScrollIntentRef]);

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

    const opening = prevKeyboardOpenRef.current === false && keyboardOpen;
    const closing = prevKeyboardOpenRef.current === true && !keyboardOpen;

    if (opening) {
      preKeyboardGeometryRef.current =
        geometryRef.current ?? captureTranscriptGeometry(node);
    }

    const before =
      closing && preKeyboardGeometryRef.current
        ? preKeyboardGeometryRef.current
        : geometryRef.current ?? captureTranscriptGeometry(node);
    const restoreTarget: TranscriptGeometry = {
      ...before,
      atBottom: before.atBottom,
    };

    prevKeyboardOpenRef.current = keyboardOpen;
    layoutTransitionRef.current = true;
    ignoreScrollIntentRef.current = true;

    const cancelSettle = afterTranscriptLayoutSettles(
      node,
      restoreTarget,
      () => {
        restoreTranscriptGeometry(node, restoreTarget);
        if (closing && restoreTarget.atBottom) onRestoreLiveEdge?.();
        geometryRef.current = captureTranscriptGeometry(node);
        ignoreScrollIntentRef.current = false;
        layoutTransitionRef.current = false;
        if (closing) preKeyboardGeometryRef.current = null;
      },
      { ignoreProgrammaticScroll: ignoreScrollIntentRef, pinnedRef },
    );

    return () => {
      cancelSettle();
      ignoreScrollIntentRef.current = false;
      layoutTransitionRef.current = false;
    };
  }, [
    keyboardOpen,
    ignoreScrollIntentRef,
    layoutTransitionRef,
    onRestoreLiveEdge,
    threadRef,
  ]);

  useEffect(() => {
    const composer = composerRef.current;
    const node = threadRef.current;
    if (!composer || !node || typeof ResizeObserver === "undefined") return;

    composerHeightRef.current = composer.offsetHeight;
    const observer = new ResizeObserver(() => {
      const nextHeight = composer.offsetHeight;
      if (nextHeight === composerHeightRef.current) return;
      composerHeightRef.current = nextHeight;

      if (!pinnedRef.current) return;

      ignoreScrollIntentRef.current = true;
      pinTranscriptToLiveEdge(node);
      geometryRef.current = captureTranscriptGeometry(node);
      ignoreScrollIntentRef.current = false;
    });
    observer.observe(composer);
    return () => observer.disconnect();
  }, [composerRef, pinnedRef, threadRef, ignoreScrollIntentRef]);

  const surfaceStyle = sessionSurfaceStyle(
    innerHeight,
    visualViewportHeight,
    keyboardOpen,
  );

  return { keyboardOpen, keyboardHeight, surfaceStyle };
}
