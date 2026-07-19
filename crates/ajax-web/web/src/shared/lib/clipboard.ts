// Clipboard write with an execCommand fallback for plain-http LAN origins,
// where navigator.clipboard does not exist.

/** Copy to clipboard; returns true when the native clipboard accepted it. */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // NotAllowedError when backgrounded on iOS, SecurityError in some contexts.
  }
  // navigator.clipboard only exists on secure origins; the cockpit is often
  // served over plain LAN http, where the deprecated execCommand path is the
  // only way to write the clipboard. It needs a real focused selection.
  try {
    const scratch = document.createElement("textarea");
    scratch.value = text;
    scratch.setAttribute("readonly", "");
    scratch.style.position = "fixed";
    scratch.style.opacity = "0";
    document.body.appendChild(scratch);
    scratch.focus();
    scratch.select();
    const copied = document.execCommand("copy");
    scratch.remove();
    return copied;
  } catch {
    return false;
  }
}
