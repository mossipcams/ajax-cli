/** iOS hold-to-delete needs deletable content in the helper textarea. */
export const BACKSPACE_SENTINEL = "\u200B";

export function seedBackspaceSentinel(input: HTMLTextAreaElement | null): void {
  if (input && !input.value.includes(BACKSPACE_SENTINEL)) {
    input.value = BACKSPACE_SENTINEL;
  }
}

/** Stable focus listener identity for hardenMobileTextarea ↔ effect cleanup. */
export function seedSentinelFromFocus(event: Event): void {
  const input = event.currentTarget;
  seedBackspaceSentinel(input instanceof HTMLTextAreaElement ? input : null);
}
