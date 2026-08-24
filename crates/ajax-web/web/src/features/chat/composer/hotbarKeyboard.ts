import type { MouseEvent } from "react";

const COMPOSER_HOTBAR_SELECTOR = "[data-testid='session-composer-hotbar']";

const COMPOSER_HOTBAR_INTERACTIVE_SELECTOR =
  "button, a, input, textarea, select, [role='button'], summary";

export function isComposerHotbarDeadSpace(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (!target.closest(COMPOSER_HOTBAR_SELECTOR)) return false;
  return !target.closest(COMPOSER_HOTBAR_INTERACTIVE_SELECTOR);
}

/** iOS Safari dismisses the keyboard unless default is cancelled before focus moves. */
export function retainComposerKeyboardOnHotbarCapture(event: Event): void {
  if (!isComposerHotbarDeadSpace(event.target)) return;
  if (event.cancelable) event.preventDefault();
}

/** Slash-menu pattern: keep focus in the composer while the control still receives click. */
export function preventComposerHotbarFocusSteal(event: MouseEvent): void {
  event.preventDefault();
}

const HOTBAR_CAPTURE_OPTIONS: AddEventListenerOptions = { capture: true, passive: false };

/** Registers capture touchstart/pointerdown retention on hotbar dead space. */
export function attachComposerHotbarKeyboardRetention(hotbar: HTMLElement): () => void {
  const handler = (event: Event) => retainComposerKeyboardOnHotbarCapture(event);
  hotbar.addEventListener("touchstart", handler, HOTBAR_CAPTURE_OPTIONS);
  hotbar.addEventListener("pointerdown", handler, HOTBAR_CAPTURE_OPTIONS);
  return () => {
    hotbar.removeEventListener("touchstart", handler, HOTBAR_CAPTURE_OPTIONS);
    hotbar.removeEventListener("pointerdown", handler, HOTBAR_CAPTURE_OPTIONS);
  };
}
