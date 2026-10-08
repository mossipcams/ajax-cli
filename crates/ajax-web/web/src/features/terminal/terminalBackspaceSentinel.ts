export const BACKSPACE_SENTINEL = "\u200B";

export const seedBackspaceSentinel = (input: HTMLTextAreaElement | null) => {
  if (input && !input.value.includes(BACKSPACE_SENTINEL)) {
    input.value = BACKSPACE_SENTINEL;
  }
};

export const seedSentinelFromFocus = (event: Event) => {
  const input = event.currentTarget;
  seedBackspaceSentinel(input instanceof HTMLTextAreaElement ? input : null);
};
