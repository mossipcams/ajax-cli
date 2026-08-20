import { useMemo, type RefObject } from "react";
import type { SpeechInsert } from "@/shared/lib/speechInsertLedger";
import { useSpeechInput } from "@/shared/hooks/useSpeechInput";

export type ChatSpeechDeps = {
  handle: string;
  draftRef: RefObject<string>;
  setDraft: (value: string) => void;
};

/** Chat-owned speech adapter: inserts finalized STT text into the composer draft. */
export function useChatSpeech({ handle, draftRef, setDraft }: ChatSpeechDeps) {
  const adapter = useMemo(
    () => ({
      insertDelta: (delta: string) => {
        const current = draftRef.current;
        const separator = current && !/\s$/.test(current) ? " " : "";
        const textToInsert = `${separator}${delta}`;
        const next = `${current}${textToInsert}`;
        draftRef.current = next;
        setDraft(next);
        return { ok: true, record: { text: textToInsert, bracketed: false } satisfies SpeechInsert };
      },
      undoInserts: (records: readonly SpeechInsert[]) => {
        const charCount = records.reduce((sum, record) => sum + record.text.length, 0);
        if (charCount === 0) return;
        const current = draftRef.current;
        const next = current.slice(0, Math.max(0, current.length - charCount));
        draftRef.current = next;
        setDraft(next);
      },
    }),
    [draftRef, setDraft],
  );

  return useSpeechInput(handle, adapter);
}
