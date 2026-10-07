import { looksLikeHttpUrl, readPasteText } from "@/shared/lib/clipboard";
import { BACKSPACE_SENTINEL } from "./terminalBackspaceSentinel";

export function deleteInputPayload(inputType: string): string | null {
  if (inputType === "deleteWordBackward") return "\x17";
  if (inputType === "deleteContentBackward" || inputType === "deleteContentForward") {
    return "\x7f";
  }
  return null;
}

export function pasteTextFromBeforeInput(event: InputEvent): string | null {
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
  if (fromInsert && !fromPaste && !looksLikeHttpUrl(text)) return null;
  return text;
}

export function pasteRawFromExpectValue(value: string): string {
  return value.replaceAll(BACKSPACE_SENTINEL, "");
}

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
    }
  }

  const readText = clipboard.readText;
  if (!readText) return null;
  return await readText.call(clipboard);
}
