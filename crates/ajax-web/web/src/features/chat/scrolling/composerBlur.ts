import type { MouseEvent, PointerEvent, RefObject } from "react";

/** Regions where tap-dismiss must not blur the session composer or close the keyboard. */
export const COMPOSER_KEYBOARD_DISMISS_EXEMPT_SELECTOR =
  "button, a, input, textarea, select, [role='button'], summary, [data-testid='session-composer-hotbar']";

const COMPOSER_HOTBAR_SELECTOR = "[data-testid='session-composer-hotbar']";

const COMPOSER_HOTBAR_INTERACTIVE_SELECTOR =
  "button, a, input, textarea, select, [role='button'], summary";

export function isComposerKeyboardDismissTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return !target.closest(COMPOSER_KEYBOARD_DISMISS_EXEMPT_SELECTOR);
}

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

/** iOS keyboard retention for terminal toolbar: dead space and pointerdown on keys. */
export function retainToolbarKeyboardOnCapture(root: HTMLElement, event: Event): void {
  if (!(event.target instanceof Node) || !root.contains(event.target)) return;
  const interactive =
    event.target instanceof HTMLElement &&
    event.target.closest(COMPOSER_HOTBAR_INTERACTIVE_SELECTOR);
  // touchstart preventDefault on a key swallows the click; pointerdown still
  // keeps the xterm helper focused via onToolbarPointerDown.
  if (interactive && event.type === "touchstart") return;
  if (event.cancelable) event.preventDefault();
}

/** Registers capture touchstart/pointerdown retention on a terminal toolbar root. */
export function attachToolbarKeyboardRetention(root: HTMLElement): () => void {
  const handler = (event: Event) => retainToolbarKeyboardOnCapture(root, event);
  root.addEventListener("touchstart", handler, HOTBAR_CAPTURE_OPTIONS);
  root.addEventListener("pointerdown", handler, HOTBAR_CAPTURE_OPTIONS);
  return () => {
    root.removeEventListener("touchstart", handler, HOTBAR_CAPTURE_OPTIONS);
    root.removeEventListener("pointerdown", handler, HOTBAR_CAPTURE_OPTIONS);
  };
}

/** Blurs the composer when tapping non-interactive chat chrome above the hotbar. */
export function blurComposerOnPointerDown(
  event: PointerEvent<HTMLElement>,
  composerRef: RefObject<HTMLTextAreaElement | null>,
) {
  if (!isComposerKeyboardDismissTarget(event.target)) return;
  composerRef.current?.blur();
}
