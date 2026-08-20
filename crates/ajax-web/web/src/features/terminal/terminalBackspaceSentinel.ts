// iOS only starts its hold-to-delete repeat loop when the focused field has
// deletable content, so the xterm helper textarea always carries a sentinel.
export const BACKSPACE_SENTINEL = "\u200B";

export const seedBackspaceSentinel = (input: HTMLTextAreaElement | null) => {
  if (input && !input.value.includes(BACKSPACE_SENTINEL)) {
    input.value = BACKSPACE_SENTINEL;
  }
};

// Module scope on purpose: registered from hardenMobileTextarea and removed in
// the effect cleanup, which see different render closures. One stable identity
// is the only way both sides name the same function.
export const seedSentinelFromFocus = (event: Event) => {
  const input = event.currentTarget;
  seedBackspaceSentinel(input instanceof HTMLTextAreaElement ? input : null);
};
