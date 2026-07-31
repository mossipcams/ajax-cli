// Clipboard write with an execCommand fallback for plain-http LAN origins,
// where navigator.clipboard does not exist.

/** First http(s) URL at the start of trimmed text, or null. */
function extractHttpUrl(text: string): string | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const match = trimmed.match(/^https?:\/\/[^\s<>"{}|\\^`[\]]+/i);
  return match?.[0] ?? null;
}

function firstUriListLine(uriList: string): string {
  for (const line of uriList.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed && !trimmed.startsWith("#")) return trimmed;
  }
  return "";
}

function extractHttpUrlFromUriList(uriList: string): string | null {
  const line = firstUriListLine(uriList);
  return line ? extractHttpUrl(line) : null;
}

function extractHttpUrlFromHtml(html: string): string | null {
  if (!html) return null;
  const hrefMatch = html.match(/\bhref\s*=\s*["']([^"']+)["']/i);
  if (!hrefMatch) return null;
  return extractHttpUrl(hrefMatch[1]);
}

/**
 * Read native paste payload. Prefers an http(s) URL from plain text, uri-list,
 * or an HTML href when plain is empty or only a link title; never returns raw
 * HTML markup.
 */
export function readPasteText(data: DataTransfer | null): string {
  if (!data) return "";
  const plain = data.getData("text/plain");
  const uriList = data.getData("text/uri-list");
  const html = data.getData("text/html");
  const plainTrim = plain.trim();
  const fromUri = extractHttpUrlFromUriList(uriList);
  const fromHtml = extractHttpUrlFromHtml(html);

  if (plainTrim) {
    // Keep full plain when it already starts with a URL (may include trailing text).
    if (/^https?:\/\//i.test(plainTrim)) return plainTrim;
    // Link title + rich URL → prefer the URL.
    return fromUri ?? fromHtml ?? plainTrim;
  }

  return fromUri ?? fromHtml ?? firstUriListLine(uriList);
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
