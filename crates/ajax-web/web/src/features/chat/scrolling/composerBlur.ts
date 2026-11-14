import type { PointerEvent, RefObject } from "react";

export const COMPOSER_KEYBOARD_DISMISS_EXEMPT_SELECTOR =
  "button, a, input, textarea, select, [role='button'], summary, [data-testid='session-composer-hotbar']";

const COMPOSER_HOTBAR_INTERACTIVE_SELECTOR =
  "button, a, input, textarea, select, [role='button'], summary";

export function isComposerKeyboardDismissTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return !target.closest(COMPOSER_KEYBOARD_DISMISS_EXEMPT_SELECTOR);
}

const HOTBAR_CAPTURE_OPTIONS: AddEventListenerOptions = { capture: true, passive: false };

export function retainToolbarKeyboardOnCapture(root: HTMLElement, event: Event): void {
  if (!(event.target instanceof Node) || !root.contains(event.target)) return;
  const interactive =
    event.target instanceof HTMLElement &&
    event.target.closest(COMPOSER_HOTBAR_INTERACTIVE_SELECTOR);
  if (interactive && event.type === "touchstart") return;
  if (event.cancelable) event.preventDefault();
}

export function attachToolbarKeyboardRetention(root: HTMLElement): () => void {
  const handler = (event: Event) => retainToolbarKeyboardOnCapture(root, event);
  root.addEventListener("touchstart", handler, HOTBAR_CAPTURE_OPTIONS);
  root.addEventListener("pointerdown", handler, HOTBAR_CAPTURE_OPTIONS);
  return () => {
    root.removeEventListener("touchstart", handler, HOTBAR_CAPTURE_OPTIONS);
    root.removeEventListener("pointerdown", handler, HOTBAR_CAPTURE_OPTIONS);
  };
}

export function blurComposerOnPointerDown(
  event: PointerEvent<HTMLElement>,
  composerRef: RefObject<HTMLTextAreaElement | null>,
) {
  if (!isComposerKeyboardDismissTarget(event.target)) return;
  composerRef.current?.blur();
}
