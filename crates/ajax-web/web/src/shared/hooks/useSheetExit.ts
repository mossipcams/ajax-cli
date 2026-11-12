import { useCallback, useRef } from "react";

const EXIT_DURATION_MS = 180;
const EXIT_EASING = "cubic-bezier(0.22, 1, 0.36, 1)";

export function useSheetExit<Card extends HTMLElement = HTMLDivElement>(onClose: () => void) {
  const scrimRef = useRef<HTMLDivElement>(null);
  const cardRef = useRef<Card>(null);
  const closingRef = useRef(false);

  const requestClose = useCallback(() => {
    if (closingRef.current) return;
    closingRef.current = true;

    const notify = () => {
      onClose();
      closingRef.current = false;
    };
    const scrim = scrimRef.current;
    const card = cardRef.current;
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;
    if (!card || typeof card.animate !== "function" || reducedMotion) {
      notify();
      return;
    }

    const timing: KeyframeAnimationOptions = {
      duration: EXIT_DURATION_MS,
      easing: EXIT_EASING,
      fill: "forwards",
    };
    const dragOffset = /translateY\(\s*([\d.]+)px\s*\)/.exec(card.style.transform);
    const cardKeyframes: Keyframe[] =
      dragOffset && Number(dragOffset[1]) > 0
        ? [{ transform: "translateY(100%)" }]
        : [{ opacity: 0, transform: "translateY(16px)" }];
    if (scrim) {
      scrim.style.pointerEvents = "none";
      scrim.animate([{ opacity: 0 }], timing);
    }
    const finished = card.animate(cardKeyframes, timing).finished;
    finished.then(notify, notify);
  }, [onClose]);

  return { scrimRef, cardRef, requestClose };
}
