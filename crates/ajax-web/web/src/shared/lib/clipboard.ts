export function looksLikeHttpUrl(text: string): boolean {
  return /^https?:\/\//i.test(text.trim());
}

export function readPasteText(data: DataTransfer | null): string {
  if (!data) return "";
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

export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
  }
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
