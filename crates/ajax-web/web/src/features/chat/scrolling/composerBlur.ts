import type { PointerEvent, RefObject } from "react";

/** Blurs the composer when tapping non-interactive chat chrome. */
export function blurComposerOnPointerDown(
  event: PointerEvent<HTMLElement>,
  composerRef: RefObject<HTMLTextAreaElement | null>,
) {
  const target = event.target as HTMLElement;
  if (target.closest("button, a, input, textarea, select, [role='button'], summary")) return;
  composerRef.current?.blur();
}
