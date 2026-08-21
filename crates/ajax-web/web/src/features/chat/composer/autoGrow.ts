/** Grow the composer to its content. CSS `max-height` caps it, after which the
 * textarea scrolls internally — a one-row box that scrolls is unusable on a
 * phone, which is where this surface lives.
 *
 * Typing forward is the hot path and only ever needs to grow, so it skips the
 * reset entirely; the `height = "auto"` reset — which forces a synchronous
 * reflow on every keystroke — runs only when the text may have gotten shorter. */
export function autoGrow(node: HTMLTextAreaElement, shrank: boolean) {
  if (shrank) node.style.height = "auto";
  else if (node.scrollHeight <= node.clientHeight) return;
  node.style.height = `${node.scrollHeight}px`;
}
