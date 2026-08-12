// Pure paste / delete helpers for the task terminal helper-textarea.
// Kept out of TaskTerminal.tsx so that file stays under the LOC hard limit.

import { looksLikeHttpUrl, readPasteText } from "@/shared/lib/clipboard";
import { BACKSPACE_SENTINEL } from "./terminalBackspaceSentinel";

/** Map iOS/WebKit beforeinput delete types to PTY key bytes. */
export function deleteInputPayload(inputType: string): string | null {
  if (inputType === "deleteWordBackward") return "\x17";
  if (inputType === "deleteContentBackward" || inputType === "deleteContentForward") {
    return "\x7f";
  }
  return null;
}

/**
 * Text to send from a helper-textarea beforeinput paste gesture, or null to
 * ignore (normal typing, empty payload, non-URL insertText).
 */
export function pasteTextFromBeforeInput(event: InputEvent): string | null {
  // iOS keyboard "Paste" / QuickType link often uses beforeinput with the URL
  // in event.data and an empty ClipboardEvent.clipboardData. WebKit may also
  // deliver that gesture as insertText / insertReplacementText (not
  // insertFromPaste) with the full URL in event.data in one shot.
  const fromPaste =
    event.inputType === "insertFromPaste" ||
    event.inputType === "insertFromPasteAsQuotation";
  const fromInsert =
    event.inputType === "insertText" || event.inputType === "insertReplacementText";
  if (!fromPaste && !fromInsert) return null;

  const text =
    (event.dataTransfer ? readPasteText(event.dataTransfer) : "") ||
    (event.data ?? "").trim();
  if (!text) return null;
  // insertText is also normal typing (one codepoint per event). Only treat it
  // as paste when the payload is a full http(s) URL.
  if (fromInsert && !fromPaste && !looksLikeHttpUrl(text)) return null;
  return text;
}

/** Strip the ZWS backspace sentinel from a paste-expect textarea recovery. */
export function pasteRawFromExpectValue(value: string): string {
  return value.replaceAll(BACKSPACE_SENTINEL, "");
}

/**
 * Toolbar Paste: prefer typed clipboard.read (html/uri-list) via readPasteText,
 * then fall back to readText. Returns null when nothing usable is available.
 */
export async function readToolbarPasteText(
  clipboard: Clipboard | undefined = navigator.clipboard,
): Promise<string | null> {
  if (!clipboard) return null;

  if (typeof clipboard.read === "function") {
    try {
      const items = await clipboard.read();
      const dt = new DataTransfer();
      for (const item of items) {
        for (const type of item.types) {
          if (type !== "text/plain" && type !== "text/html" && type !== "text/uri-list") {
            continue;
          }
          dt.setData(type, await (await item.getType(type)).text());
        }
      }
      const rich = readPasteText(dt);
      if (rich) return rich;
    } catch {
      // NotAllowedError / insecure context — fall through to readText.
    }
  }

  const readText = clipboard.readText;
  if (!readText) return null;
  // Empty string means "clipboard empty" (caller refocuses); null means
  // unavailable. Do not collapse "" → null.
  return await readText.call(clipboard);
}
