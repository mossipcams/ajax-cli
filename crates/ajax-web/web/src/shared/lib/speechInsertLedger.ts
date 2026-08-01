export type SpeechInsert = {
  text: string;
  bracketed: boolean;
};

export interface PrepareSpeechInsertOptions {
  hasPriorInserts: boolean;
  textStartsWithWhitespace?: boolean;
  bracketed: boolean;
}

export function prepareSpeechInsert(
  text: string,
  {
    hasPriorInserts,
    textStartsWithWhitespace = /^\s/.test(text),
    bracketed,
  }: PrepareSpeechInsertOptions,
): { textToPaste: string; record: SpeechInsert } {
  const needsLeadingSpace = hasPriorInserts && !textStartsWithWhitespace;
  const textToPaste = needsLeadingSpace ? ` ${text}` : text;
  return {
    textToPaste,
    record: { text: textToPaste, bracketed },
  };
}

/** Sum of plain insert text lengths; `bracketed` is metadata only for undo. */
export function undoPayload(records: readonly SpeechInsert[]): string {
  const charCount = records.reduce((sum, record) => sum + record.text.length, 0);
  return "\x7f".repeat(charCount);
}

export function clearSpeechInserts(records: SpeechInsert[]): void {
  records.length = 0;
}
