// Clipboard write with an execCommand fallback for plain-http LAN origins,
// where navigator.clipboard does not exist.

/** True when trimmed text is an http(s) URL (optionally with trailing path junk). */
export function looksLikeHttpUrl(text: string): boolean {
  return /^https?:\/\//i.test(text.trim());
}

/**
 * Read native paste payload. Prefers an http(s) URL from plain text, uri-list,
 * or an HTML href when plain is empty or only a link title; never returns raw
 * HTML markup.
 */
export function readPasteText(data: DataTransfer | null): string {
  if (!data) return "";
  // Some WebKit builds expose plain as "text" rather than "text/plain".
  const plain = (data.getData("text/plain") || data.getData("text")).trim();
  if (looksLikeHttpUrl(plain)) return plain;

  const uri =
    data
      .getData("text/uri-list")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => line && !line.startsWith("#")) ?? "";
  const html = data.getData("text/html");
  const href =
    html.match(/\bhref\s*=\s*(?:"([^"]+)"|'([^']+)'|([^\s>]+))/i)?.slice(1).find(Boolean)?.trim() ??
    "";
  const richUrl = [uri, href].find((candidate) => looksLikeHttpUrl(candidate));

  if (plain) return richUrl ?? plain;
  return richUrl ?? uri;
}

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
