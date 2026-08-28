import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import {
  isLayoutExpandedBeyondStaleVisualViewport,
  isSessionKeyboardOpen,
  layoutViewportShrinksWithKeyboard,
  sessionKeyboardPadding,
} from "@/shared/lib/sessionViewport";

interface MobileKeyboardSnapshot {
  isMobile: boolean;
  keyboardOpen: boolean;
  keyboardHeight: number;
  visualViewportHeight: number;
  innerHeight: number;
}

function createKeyboardStore() {
  const initialIsMobile =
    typeof window !== "undefined" && window.matchMedia?.("(pointer: coarse)").matches;
  let snapshot: MobileKeyboardSnapshot = {
    isMobile: initialIsMobile,
    keyboardOpen: false,
    keyboardHeight: 0,
    visualViewportHeight: typeof window !== "undefined" ? window.innerHeight : 0,
    innerHeight: typeof window !== "undefined" ? window.innerHeight : 0,
  };
  const listeners = new Set<() => void>();
  return {
    getSnapshot: () => snapshot,
    update: (partial: Partial<MobileKeyboardSnapshot>) => {
      snapshot = { ...snapshot, ...partial };
      listeners.forEach((l) => l());
    },
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

type KeyboardStore = ReturnType<typeof createKeyboardStore>;

function readSafeBottomPx(): number {
  if (typeof document === "undefined") return 0;
  return (
    parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--safe-area-bottom"),
    ) || 0
  );
}

function isFormControlFocused(): boolean {
  const active = document.activeElement;
  if (!active) return false;
  const tag = active.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

/** Session-scoped soft-keyboard metrics; does not toggle html.keyboard-open. */
export function useMobileKeyboard() {
  const [store] = useState<KeyboardStore>(() => createKeyboardStore());
  const state = useSyncExternalStore(store.subscribe, store.getSnapshot);

  const rafRef = useRef(0);
  const stableCountRef = useRef(0);
  const lastOcclusionRef = useRef(0);
  const fullHeightRef = useRef(0);
  /** ponytail: tap-dismiss often leaves vv shrunk with no resize; ignore until refocus. */
  const ignoreShrinkRef = useRef(false);
  /** True when this keyboard session shrunk innerHeight with vv (PWA / iOS 26). */
  const layoutShrinksWithKeyboardRef = useRef(false);
  const lastReportedVvHRef = useRef(0);

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mql = window.matchMedia("(pointer: coarse)");
    const onChange = () => {
      if (mql.matches) {
        store.update({ isMobile: true });
      } else {
        store.update({
          isMobile: false,
          keyboardOpen: false,
          keyboardHeight: 0,
        });
      }
    };
    mql.addEventListener?.("change", onChange);
    return () => mql.removeEventListener?.("change", onChange);
  }, [store]);

  useEffect(() => {
    if (!state.isMobile) return;
    const vv = window.visualViewport;
    if (!vv) return;

    fullHeightRef.current = Math.max(window.innerHeight, vv.height);

    let lastOpen = false;
    let lastPadding = 0;

    const measure = () => {
      const innerHeight = window.innerHeight;
      const currentVvH = vv.height;

      if (currentVvH > fullHeightRef.current - 50) {
        fullHeightRef.current = Math.max(fullHeightRef.current, currentVvH);
      }

      if (ignoreShrinkRef.current) {
        if (currentVvH > lastReportedVvHRef.current + 1) {
          lastReportedVvHRef.current = currentVvH;
          if (currentVvH >= fullHeightRef.current - 50) {
            fullHeightRef.current = Math.max(fullHeightRef.current, currentVvH);
            ignoreShrinkRef.current = false;
          }
          store.update({
            keyboardOpen: false,
            keyboardHeight: 0,
            visualViewportHeight: currentVvH,
            innerHeight,
          });
        } else if (
          layoutShrinksWithKeyboardRef.current &&
          isLayoutExpandedBeyondStaleVisualViewport(innerHeight, currentVvH)
        ) {
          layoutShrinksWithKeyboardRef.current = false;
          ignoreShrinkRef.current = false;
          fullHeightRef.current = Math.max(fullHeightRef.current, innerHeight, currentVvH);
          lastReportedVvHRef.current = currentVvH;
          store.update({
            keyboardOpen: false,
            keyboardHeight: 0,
            visualViewportHeight: currentVvH,
            innerHeight,
          });
        }
        return fullHeightRef.current - currentVvH;
      }

      const open = isSessionKeyboardOpen(fullHeightRef.current, currentVvH);
      const safeBottom = readSafeBottomPx();
      const padding = sessionKeyboardPadding(innerHeight, currentVvH, open, safeBottom);

      if (
        open &&
        layoutViewportShrinksWithKeyboard(innerHeight, currentVvH)
      ) {
        layoutShrinksWithKeyboardRef.current = true;
      } else if (!open) {
        layoutShrinksWithKeyboardRef.current = false;
      }

      if (
        open &&
        layoutShrinksWithKeyboardRef.current &&
        isLayoutExpandedBeyondStaleVisualViewport(innerHeight, currentVvH)
      ) {
        layoutShrinksWithKeyboardRef.current = false;
        ignoreShrinkRef.current = true;
        lastOpen = false;
        lastPadding = 0;
        stableCountRef.current = 0;
        lastReportedVvHRef.current = currentVvH;
        fullHeightRef.current = Math.max(fullHeightRef.current, innerHeight);
        store.update({
          keyboardOpen: false,
          keyboardHeight: 0,
          visualViewportHeight: currentVvH,
          innerHeight,
        });
        return fullHeightRef.current - currentVvH;
      }

      if (open !== lastOpen || padding !== lastPadding) {
        lastOpen = open;
        lastPadding = padding;
        stableCountRef.current = 0;
        lastReportedVvHRef.current = currentVvH;
        store.update({
          keyboardOpen: open,
          keyboardHeight: padding,
          visualViewportHeight: currentVvH,
          innerHeight,
        });
      }

      return fullHeightRef.current - currentVvH;
    };

    const MAX_POLL_FRAMES = 20;
    const STABLE_THRESHOLD = 3;
    const startPolling = () => {
      cancelAnimationFrame(rafRef.current);
      stableCountRef.current = 0;
      let frameCount = 0;
      const poll = () => {
        frameCount++;
        const occlusion = measure();
        if (Math.abs(occlusion - lastOcclusionRef.current) < 1) {
          stableCountRef.current++;
        } else {
          stableCountRef.current = 0;
        }
        lastOcclusionRef.current = occlusion;
        if (stableCountRef.current < STABLE_THRESHOLD && frameCount < MAX_POLL_FRAMES) {
          rafRef.current = requestAnimationFrame(poll);
        }
      };
      rafRef.current = requestAnimationFrame(poll);
    };

    const handleViewportChange = () => {
      measure();
      startPolling();
    };

    const handleFocusIn = (e: FocusEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
        ignoreShrinkRef.current = false;
        layoutShrinksWithKeyboardRef.current = false;
        startPolling();
      }
    };

    const handleFocusOut = () => {
      requestAnimationFrame(() => {
        if (isFormControlFocused()) return;
        ignoreShrinkRef.current = true;
        layoutShrinksWithKeyboardRef.current = layoutViewportShrinksWithKeyboard(
          window.innerHeight,
          vv.height,
        );
        lastOpen = false;
        lastPadding = 0;
        lastReportedVvHRef.current = vv.height;
        store.update({
          keyboardOpen: false,
          keyboardHeight: 0,
          visualViewportHeight: vv.height,
          innerHeight: window.innerHeight,
        });
      });
    };

    let orientTimer: ReturnType<typeof setTimeout> | null = null;
    const handleOrientationChange = () => {
      fullHeightRef.current = 0;
      if (orientTimer) clearTimeout(orientTimer);
      orientTimer = setTimeout(() => {
        fullHeightRef.current = Math.max(window.innerHeight, vv.height);
        measure();
      }, 500);
    };

    measure();
    lastReportedVvHRef.current = vv.height;
    vv.addEventListener("resize", handleViewportChange);
    vv.addEventListener("scroll", handleViewportChange);
    window.addEventListener("resize", handleViewportChange);
    document.addEventListener("focusin", handleFocusIn);
    document.addEventListener("focusout", handleFocusOut);
    window.addEventListener("orientationchange", handleOrientationChange);
    return () => {
      cancelAnimationFrame(rafRef.current);
      if (orientTimer) clearTimeout(orientTimer);
      vv.removeEventListener("resize", handleViewportChange);
      vv.removeEventListener("scroll", handleViewportChange);
      window.removeEventListener("resize", handleViewportChange);
      document.removeEventListener("focusin", handleFocusIn);
      document.removeEventListener("focusout", handleFocusOut);
      window.removeEventListener("orientationchange", handleOrientationChange);
    };
  }, [state.isMobile, store]);

  return {
    isMobile: state.isMobile,
    keyboardOpen: state.keyboardOpen,
    keyboardHeight: state.keyboardHeight,
    visualViewportHeight: state.visualViewportHeight,
    innerHeight: state.innerHeight,
  };
}
